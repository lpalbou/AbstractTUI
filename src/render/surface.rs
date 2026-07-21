//! Surface: an owned 2D cell buffer with a glyph pool and a link table.
//!
//! All write paths maintain one structural invariant the rest of the
//! pipeline (compositor repair pass, diff, presenter, VT model) relies on:
//!
//! > A wide glyph is exactly a leader cell (width 2) immediately followed
//! > on the same row by one continuation cell mirroring the leader's
//! > style. Continuations never appear anywhere else, and a leader never
//! > sits in the last column.
//!
//! Clobbering either half of a pair blanks the orphan half (space, style
//! kept) — exactly what a real terminal does when you overwrite half a
//! CJK glyph. The repair helpers (`release_edges`, `sever_pairs_at_edges`)
//! centralize this so every op (set / fill / draw / blit / scroll) shares
//! one correctness story.
//!
//! Surfaces also accumulate their own damage (surface-local rects, slightly
//! over-approximated to cover pair repairs). The compositor drains it;
//! fine-grained reactivity means "one changed cell" arrives here as a tiny
//! rect, never a full-frame redraw.

use crate::base::{Point, Rect, Size};

use super::cell::{Cell, Glyph, GlyphPool};
use super::style::Style;
use crate::text;

/// Beyond this many damage rects the list collapses to its union: a long
/// list means scattered writes, and per-rect bookkeeping stops paying for
/// itself (the diff would visit most rows anyway).
const DAMAGE_CAP: usize = 32;

/// Link-table cap (RT1-14): ids are u16 and 0 means "no link", so 65535
/// URIs is the hard ceiling; past it new links are dropped (plain text)
/// and counted — a wrapped id would silently point text at the WRONG URI,
/// which is strictly worse than no link.
const LINK_TABLE_CAP: usize = u16::MAX as usize;

/// An owned cell grid: the thing everything draws INTO.
///
/// Drawing vocabulary [C8, frozen]: **[`Surface::draw_text`] is the
/// canonical rich-draw call** for anyone holding a `Surface` — grapheme
/// segmentation, wide-pair maintenance, clipping and the [`Style`] patch
/// language all live there. The `ui::Canvas`/`ui::StyledCanvas` methods
/// (`put`/`print`/`print_styled`) exist for WIDGET code drawing through
/// a `&mut dyn` canvas; on a `Surface` they all funnel into `draw_text`.
/// If you have the concrete type, call `draw_text`.
///
/// ```
/// use abstracttui::base::{Rgba, Size};
/// use abstracttui::render::{snapshot, Cell, Style, Surface};
///
/// let mut s = Surface::new(Size::new(20, 3), Cell::EMPTY);
/// let ink = Style::new().fg(Rgba::rgb(220, 60, 60)).bold();
/// let advanced = s.draw_text(1, 1, "hello 世界", ink);
/// assert_eq!(advanced, 10); // columns, not chars: CJK glyphs are 2 wide
/// assert!(snapshot(&s).contains("hello 世界"));
/// ```
pub struct Surface {
    // Fields are pub(super) for the sibling split files (surface_ops.rs
    // scroll/resize) — the render module family shares the invariant
    // discipline; everything outside goes through methods.
    pub(super) size: Size,
    pub(super) cells: Vec<Cell>,
    pool: GlyphPool,
    /// Hyperlink URIs; cell id N resolves to `links[N-1]` (0 = none).
    links: Vec<Box<str>>,
    /// Links refused at [`LINK_TABLE_CAP`] — the labeled degradation
    /// counter twin of `GlyphPool::dropped`.
    links_dropped: u32,
    damage: Vec<Rect>,
}

impl Surface {
    /// A surface of `size` filled with `fill` (use [`Cell::EMPTY`] for a
    /// see-through ground). Negative dimensions clamp to zero.
    pub fn new(size: Size, fill: Cell) -> Surface {
        let size = Size::new(size.w.max(0), size.h.max(0));
        let mut s = Surface {
            size,
            cells: vec![Cell::EMPTY; (size.w * size.h).max(0) as usize],
            pool: GlyphPool::default(),
            links: Vec::new(),
            links_dropped: 0,
            damage: Vec::new(),
        };
        // One shared fill path: wide fills tile as pairs, continuations are
        // rejected, damage collapses to the full bounds.
        if fill != Cell::EMPTY {
            s.fill_rect(s.bounds(), fill);
        }
        s.damage_all();
        s
    }

    /// Grid dimensions in cells.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Columns.
    pub fn width(&self) -> i32 {
        self.size.w
    }

    /// Rows.
    pub fn height(&self) -> i32 {
        self.size.h
    }

    /// The whole grid as a rect at origin (surface-local coordinates).
    pub fn bounds(&self) -> Rect {
        Rect::from_size(self.size)
    }

    /// This surface's glyph pool (diagnostics: `len`/`dropped` counters).
    pub fn pool(&self) -> &GlyphPool {
        &self.pool
    }

    /// Interns a hyperlink URI, returning its cell id (dedup'd). Returns 0
    /// (no link, rendered as plain text) when the id space is exhausted —
    /// the drop is counted in [`Surface::links_dropped`], never wrapped
    /// (a wrapped id would mislink, which is worse than dropping).
    pub fn register_link(&mut self, uri: &str) -> u16 {
        if let Some(i) = self.links.iter().position(|u| &**u == uri) {
            return (i + 1) as u16;
        }
        if self.links.len() >= LINK_TABLE_CAP {
            self.links_dropped = self.links_dropped.saturating_add(1);
            return 0;
        }
        self.links.push(uri.into());
        self.links.len() as u16
    }

    /// Resolves a cell's link id back to its URI (0 = no link = `None`).
    pub fn link_uri(&self, id: u16) -> Option<&str> {
        if id == 0 {
            None
        } else {
            self.links.get(id as usize - 1).map(|u| &**u)
        }
    }

    /// Hyperlink registrations refused at the table cap (labeled
    /// degradation; the affected text rendered without a link).
    pub fn links_dropped(&self) -> u32 {
        self.links_dropped
    }

    /// Resolves a cell's glyph text through THIS surface's pool — the one
    /// public resolution path (RT1-4: pool ids are surface-local; resolving
    /// a cell against a foreign pool is the bug class this API prevents).
    /// Empty and continuation glyphs resolve to `""`. The result borrows
    /// from the cell (inline glyphs) or the pool (spilled ones).
    pub fn glyph_str<'a>(&'a self, cell: &'a Cell) -> &'a str {
        cell.glyph.as_str(&self.pool)
    }

    // -- damage --------------------------------------------------------------

    /// Marks `rect` as needing repaint. Ordinary drawing records its own
    /// damage — reach for this only after out-of-band cell mutation.
    pub fn add_damage(&mut self, rect: Rect) {
        let rect = rect.intersect(self.bounds());
        if rect.is_empty() {
            return;
        }
        if self.damage.len() >= DAMAGE_CAP {
            let union = self.damage.drain(..).fold(rect, Rect::union);
            self.damage.push(union);
        } else {
            self.damage.push(rect);
        }
    }

    /// Collapses pending damage to "everything" (theme switch, resize).
    pub fn damage_all(&mut self) {
        self.damage.clear();
        self.damage.push(self.bounds());
    }

    /// Drains accumulated damage into `out` (appending, surface-local
    /// coordinates; the compositor's layer translates to frame space).
    pub fn take_damage(&mut self, out: &mut Vec<Rect>) {
        out.append(&mut self.damage);
    }

    /// True when a repaint is pending (drives idle-frame skipping).
    pub fn has_damage(&self) -> bool {
        !self.damage.is_empty()
    }

    /// Damage for a written span, expanded one column each side so pair
    /// repairs at the edges are covered without individual tracking.
    fn damage_span(&mut self, y: i32, x0: i32, x1: i32) {
        self.add_damage(Rect::new(x0 - 1, y, (x1 - x0) + 2, 1));
    }

    // -- cell access ---------------------------------------------------------

    pub(super) fn idx(&self, x: i32, y: i32) -> usize {
        debug_assert!(self.bounds().contains(Point::new(x, y)));
        (y * self.size.w + x) as usize
    }

    /// The cell at `(x, y)`, or `None` out of bounds (reads never panic).
    pub fn get(&self, x: i32, y: i32) -> Option<&Cell> {
        if self.bounds().contains(Point::new(x, y)) {
            Some(&self.cells[(y * self.size.w + x) as usize])
        } else {
            None
        }
    }

    /// Row slice for read paths (diff, compositor). `y` must be in bounds.
    pub(crate) fn row(&self, y: i32) -> &[Cell] {
        let start = self.idx(0, y);
        &self.cells[start..start + self.size.w as usize]
    }

    /// Sets one cell, preserving the wide-pair invariant. Wide glyphs write
    /// leader + continuation; a wide glyph that cannot fit (last column)
    /// degrades to a styled blank. Raw continuation cells are rejected the
    /// same way — pairing is this module's job, not the caller's.
    pub fn set(&mut self, x: i32, y: i32, cell: Cell) {
        let (x0, x1) = self.set_quiet(x, y, cell);
        if x1 > x0 {
            self.damage_span(y, x0, x1);
        }
    }

    /// `set` without damage recording, for bulk writers that damage a
    /// bounding rect once (the mosaic bridge). Returns the written column
    /// span `[x0, x1)` (empty when clipped) — pair repairs land within
    /// ±1 column of it, which `damage_span`'s expansion covers.
    pub(crate) fn set_quiet(&mut self, x: i32, y: i32, cell: Cell) -> (i32, i32) {
        if !self.bounds().contains(Point::new(x, y)) {
            return (x, x);
        }
        if cell.is_continuation() {
            self.write_narrow(x, y, cell.blanked());
            (x, x + 1)
        } else if cell.glyph.width() >= 2 {
            if x + 1 < self.size.w {
                self.write_wide(x, y, cell);
                (x, x + 2)
            } else {
                self.write_narrow(x, y, cell.blanked());
                (x, x + 1)
            }
        } else {
            self.write_narrow(x, y, cell);
            (x, x + 1)
        }
    }

    fn write_narrow(&mut self, x: i32, y: i32, cell: Cell) {
        self.release_edges(y, x, x + 1);
        let i = self.idx(x, y);
        self.cells[i] = cell;
    }

    fn write_wide(&mut self, x: i32, y: i32, leader: Cell) {
        self.release_edges(y, x, x + 2);
        let i = self.idx(x, y);
        self.cells[i] = leader;
        self.cells[i + 1] = Cell::continuation_of(&leader);
    }

    /// Repairs pairs cut by an incoming overwrite of columns `x0..x1` in row
    /// `y`: a continuation at the left edge orphans its leader (just left of
    /// the span); a leader at the right edge orphans its continuation (just
    /// right of it). Interior pairs are fully overwritten and need nothing.
    /// Must run BEFORE the overwrite — it reads the old content.
    fn release_edges(&mut self, y: i32, x0: i32, x1: i32) {
        if x0 > 0 && self.cells[self.idx(x0, y)].is_continuation() {
            let i = self.idx(x0 - 1, y);
            self.cells[i] = self.cells[i].blanked();
        }
        if x1 < self.size.w && self.cells[self.idx(x1 - 1, y)].is_wide_leader() {
            let i = self.idx(x1, y);
            self.cells[i] = self.cells[i].blanked();
        }
    }

    /// Blanks both halves of any pair straddling the vertical edges of
    /// `region`. Used by scroll: rows move only inside the region, so a
    /// straddling pair would otherwise end up with halves from different
    /// source rows.
    pub(super) fn sever_pairs_at_edges(&mut self, region: Rect) {
        for y in region.y..region.bottom() {
            if region.x > 0 && self.cells[self.idx(region.x, y)].is_continuation() {
                let li = self.idx(region.x - 1, y);
                let ci = self.idx(region.x, y);
                self.cells[li] = self.cells[li].blanked();
                self.cells[ci] = self.cells[ci].blanked();
            }
            let last = region.right() - 1;
            if region.right() < self.size.w && self.cells[self.idx(last, y)].is_wide_leader() {
                let li = self.idx(last, y);
                let ci = self.idx(last + 1, y);
                self.cells[li] = self.cells[li].blanked();
                self.cells[ci] = self.cells[ci].blanked();
            }
        }
    }

    // -- bulk ops ------------------------------------------------------------

    /// Fills `rect` (clipped) with `cell`. Wide glyphs tile as pairs; an odd
    /// trailing column gets a styled blank — never half a glyph.
    pub fn fill_rect(&mut self, rect: Rect, cell: Cell) {
        let r = rect.intersect(self.bounds());
        if r.is_empty() {
            return;
        }
        let cell = sanitize_fill(cell);
        let wide = cell.glyph.width() >= 2;
        for y in r.y..r.bottom() {
            self.release_edges(y, r.x, r.right());
            let base = self.idx(r.x, y);
            if !wide {
                self.cells[base..base + r.w as usize].fill(cell);
            } else {
                let mut x = 0usize;
                while x + 2 <= r.w as usize {
                    self.cells[base + x] = cell;
                    self.cells[base + x + 1] = Cell::continuation_of(&cell);
                    x += 2;
                }
                if x < r.w as usize {
                    self.cells[base + x] = cell.blanked();
                }
            }
            self.damage_span(y, r.x, r.right());
        }
    }

    /// Fills the whole surface and resets the glyph pool — the only moment
    /// pool ids are provably unreferenced. A pooled fill glyph is carried
    /// across the reset by re-interning its resolved text.
    pub fn clear(&mut self, cell: Cell) {
        let mut cell = sanitize_fill(cell);
        if cell.glyph.is_pooled() {
            let s = cell.glyph.as_str(&self.pool).to_string();
            let w = cell.glyph.width() as u8;
            self.pool.clear();
            cell.glyph = Glyph::from_cluster_unchecked(&s, w, &mut self.pool);
        } else {
            self.pool.clear();
        }
        self.fill_rect(self.bounds(), cell);
        self.damage_all();
    }

    /// Draws one line of text; control clusters are stripped, zero-width
    /// clusters skipped. Returns the pen advance in columns, including
    /// columns clipped off the left edge (so callers can chain segments);
    /// drawing stops at the right edge.
    ///
    /// Style is a patch over each cell's existing paint: text drawn onto a
    /// filled panel keeps the panel's background unless the style overrides.
    pub fn draw_text(&mut self, x: i32, y: i32, s: &str, style: Style) -> i32 {
        use unicode_segmentation::UnicodeSegmentation;
        if y < 0 || y >= self.size.h {
            return 0;
        }
        let mut pen = x;
        // Written column range, damaged once at the end (fewer, tighter
        // rects than per-cluster recording).
        let mut span: Option<(i32, i32)> = None;
        let mut mark = |x0: i32, x1: i32| match &mut span {
            Some((lo, hi)) => {
                *lo = (*lo).min(x0);
                *hi = (*hi).max(x1);
            }
            None => span = Some((x0, x1)),
        };
        for cluster in s.graphemes(true) {
            let w = text::cluster_width(cluster);
            if w <= 0 {
                continue;
            }
            if pen + w > self.size.w {
                break; // right clip: neither this cluster nor any later one fits
            }
            if pen + w <= 0 {
                pen += w; // fully left of the surface: advance, draw nothing
                continue;
            }
            if pen < 0 {
                // Only a wide cluster can straddle the left edge (pen == -1,
                // w == 2). Half glyphs do not exist: the visible half is a
                // styled blank.
                let base = *self.get(0, y).expect("in bounds");
                self.write_narrow(0, y, style.apply(&base).blanked());
                mark(0, 1);
                pen += w;
                continue;
            }
            let glyph = Glyph::from_cluster_unchecked(cluster, w as u8, &mut self.pool);
            let base = *self.get(pen, y).expect("in bounds");
            let mut cell = style.apply(&base);
            cell.glyph = glyph;
            if w == 2 {
                self.write_wide(pen, y, cell);
            } else {
                self.write_narrow(pen, y, cell);
            }
            mark(pen, pen + w);
            pen += w;
        }
        if let Some((x0, x1)) = span {
            self.damage_span(y, x0, x1);
        }
        pen - x
    }

    /// Copies `src_rect` of `src` onto this surface at `dst`, clipping both
    /// ends. Pooled glyphs re-intern into this surface's pool and link ids
    /// remap through the URI table — both id spaces are surface-local and
    /// never copied raw.
    pub fn blit(&mut self, src: &Surface, src_rect: Rect, dst: Point) {
        let src_rect = src_rect.intersect(src.bounds());
        if src_rect.is_empty() {
            return;
        }
        // Clip against the destination, then shift the source origin by the
        // same amount so the two rects stay in lockstep.
        let dst_rect = Rect::new(dst.x, dst.y, src_rect.w, src_rect.h).intersect(self.bounds());
        if dst_rect.is_empty() {
            return;
        }
        let sx = src_rect.x + (dst_rect.x - dst.x);
        let sy = src_rect.y + (dst_rect.y - dst.y);

        for row in 0..dst_rect.h {
            let y = dst_rect.y + row;
            self.release_edges(y, dst_rect.x, dst_rect.right());
            for col in 0..dst_rect.w {
                let cell = src.cells[src.idx(sx + col, sy + row)];
                let adopted = self.adopt_cell(cell, src);
                let i = self.idx(dst_rect.x + col, y);
                self.cells[i] = adopted;
            }
            // Pairs cut by the source-side clip: a continuation whose leader
            // was not copied, or a leader whose continuation was not.
            let first = self.idx(dst_rect.x, y);
            if self.cells[first].is_continuation() {
                self.cells[first] = self.cells[first].blanked();
            }
            let last = self.idx(dst_rect.right() - 1, y);
            if self.cells[last].is_wide_leader() {
                self.cells[last] = self.cells[last].blanked();
            }
            self.damage_span(y, dst_rect.x, dst_rect.right());
        }
    }

    // -- compositor back door --------------------------------------------

    /// Raw cell store for the compositor's flatten loop: no clipping, no
    /// pair repair, no damage recording. The compositor composes whole
    /// (±1-expanded) spans and runs [`Surface::repair_wide_pairs`] itself;
    /// its damage list is authoritative, so surface damage bookkeeping
    /// here would only be discarded.
    pub(crate) fn put_composed(&mut self, x: i32, y: i32, cell: Cell) {
        let i = self.idx(x, y);
        self.cells[i] = cell;
    }

    /// Re-establishes the wide-pair invariant over `[x0-1, x1+1)` of row
    /// `y` after a composed write of `[x0, x1)`:
    /// - a continuation with no leader immediately left blanks;
    /// - a leader with no continuation immediately right blanks;
    /// - a valid pair re-mirrors the continuation's style from the leader
    ///   (cross-layer composition can tint one half; a terminal paints a
    ///   wide glyph with a single style, so the leader's wins — see
    ///   docs/design/render.md §2.2).
    ///
    /// The ±1 walk can touch one cell beyond the composed span; the diff
    /// expands damage by ±1 again, so those repairs are always re-scanned.
    pub(crate) fn repair_wide_pairs(&mut self, y: i32, x0: i32, x1: i32) {
        let mut x = (x0 - 1).max(0);
        let end = (x1 + 1).min(self.size.w);
        // Never start mid-pair: a continuation at the walk start whose
        // leader sits just left is a valid pair — process it as a unit.
        if x > 0
            && self.cells[self.idx(x, y)].is_continuation()
            && self.cells[self.idx(x - 1, y)].is_wide_leader()
        {
            x -= 1;
        }
        while x < end {
            let i = self.idx(x, y);
            let cell = self.cells[i];
            if cell.is_continuation() {
                // No leader claimed it (a leader match advances past its
                // continuation below), so it is an orphan.
                self.cells[i] = cell.blanked();
                x += 1;
            } else if cell.is_wide_leader() {
                if x + 1 < self.size.w && self.cells[i + 1].is_continuation() {
                    self.cells[i + 1] = Cell::continuation_of(&cell);
                    x += 2;
                } else {
                    self.cells[i] = cell.blanked();
                    x += 1;
                }
            } else {
                x += 1;
            }
        }
    }

    /// [`Surface::adopt_cell`] for the compositor (same semantics, crate
    /// visibility).
    pub(crate) fn adopt_from(&mut self, cell: Cell, src: &Surface) -> Cell {
        self.adopt_cell(cell, src)
    }

    // `debug_validate` (the RT1-4 structural oracle) lives in
    // `render/validate.rs` together with the Debug renderer — diagnostics
    // only, written against the public surface API.

    /// Rewrites a foreign cell in terms of this surface's pool and links.
    fn adopt_cell(&mut self, mut cell: Cell, src: &Surface) -> Cell {
        if cell.glyph.is_pooled() {
            let s = cell.glyph.as_str(&src.pool);
            let width = cell.glyph.width() as u8;
            cell.glyph = Glyph::from_cluster_unchecked(s, width, &mut self.pool);
        }
        if cell.link != 0 {
            cell.link = match src.link_uri(cell.link) {
                Some(uri) => self.register_link(uri),
                None => 0,
            };
        }
        cell
    }
}

/// Fill cells must be self-contained: a raw continuation as fill would break
/// the pairing invariant everywhere at once.
pub(super) fn sanitize_fill(cell: Cell) -> Cell {
    if cell.is_continuation() {
        cell.blanked()
    } else {
        cell
    }
}

// `Debug` + `debug_validate` (diagnostics) live in `render/validate.rs`.

// Unit tests live beside this file to keep it within the size budget.
#[cfg(test)]
#[path = "surface_tests.rs"]
mod tests;
