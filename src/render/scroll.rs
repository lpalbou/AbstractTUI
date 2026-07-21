//! Scroll-shift detection: recognize a pure vertical shift of a row band
//! and diff against the VIRTUALLY-shifted previous frame, so the
//! presenter can emit DECSTBM + SU/SD (~20 bytes) instead of repainting
//! every moved row (measured 19.3x byte win on a 200x60 log scroll —
//! docs/design/render.md §2.7).
//!
//! Soundness shape: detection can only choose WHICH decomposition to
//! emit; correctness never depends on it. Whatever shift is chosen, the
//! run scan compares `next` against "prev after the terminal executed
//! the shift" (moved rows read from their source row; entering rows read
//! as BCE-erased blanks), so a wrong-looking candidate degrades to more
//! runs, never to wrong pixels. The property test replays random
//! shift+mutation frames and checks the decomposition cell-by-cell.
//!
//! Bounds: DECSTBM scrolls FULL rows only (left/right margins are a
//! VT420 extension real terminals rarely enable), so detection requires
//! the damage union to span the full frame width. One shift per frame
//! (the common log/list case); nested independent regions stay on the
//! plain path.

use crate::base::Rect;

use super::cell::Cell;
use super::diff::{cells_equal, FrameDiff, Run};
use super::surface::Surface;

/// A detected vertical shift of rows `[top, bottom)` by `n` (`up` = the
/// band moved toward row 0, i.e. SU). Emitted by the presenter BEFORE the
/// accompanying runs; the runs were computed against the shifted state.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Shift {
    /// First row of the shifted band (inclusive).
    pub top: i32,
    /// One past the last row of the band (exclusive).
    pub bottom: i32,
    /// Rows moved (always > 0; direction is `up`).
    pub n: i32,
    /// `true` = band moved toward row 0 (terminal SU); `false` = SD.
    pub up: bool,
}

/// The paired output of [`FrameDiff::compute_scrolled`]: runs that are
/// only meaningful AFTER the shift executes at the terminal. The pairing
/// is structural (cycle-4 hardening item): fields are private, the only
/// emission path is `Presenter::emit_scrolled(ScrolledRuns, ..)`, and
/// plain `Presenter::emit` cannot accept one — feeding shift-relative
/// runs to a shift-less emitter (the wrong-pixels hazard) no longer
/// type-checks. Read-only accessors exist for inspection/telemetry; they
/// cannot re-enter the emission path.
#[derive(Copy, Clone, Debug)]
pub struct ScrolledRuns<'a> {
    pub(super) shift: Option<Shift>,
    pub(super) runs: &'a [Run],
}

impl<'a> ScrolledRuns<'a> {
    /// A plain (shift-less) frame in token form — what `compute_scrolled`
    /// returns when detection declines; also the adapter for callers that
    /// computed runs the ordinary way but present through the paired API.
    pub fn plain(runs: &'a [Run]) -> ScrolledRuns<'a> {
        ScrolledRuns { shift: None, runs }
    }

    /// The detected shift, if the optimization engaged.
    pub fn shift(&self) -> Option<Shift> {
        self.shift
    }

    /// Residual repaint runs (valid AFTER the shift executes).
    pub fn runs(&self) -> &'a [Run] {
        self.runs
    }

    /// True for a frame that emits nothing (no shift, no runs).
    pub fn is_empty(&self) -> bool {
        self.shift.is_none() && self.runs.is_empty()
    }
}

/// Detection floor: a band shorter than this cannot save enough bytes to
/// beat its own control sequences + cursor invalidation.
const MIN_BAND_H: i32 = 8;
/// Byte-win guard: the shift must make at least this many moved rows
/// diff-clean, or plain repaint wins.
const MIN_SAVED_ROWS: i32 = 4;
/// Shift candidates examined per direction (from fingerprint anchors).
const MAX_CANDIDATES: usize = 4;

/// What the terminal holds in a row vacated by a scroll: BCE-erased cells
/// under the presenter's pre-scroll `SGR 0` — default colors, no glyph.
/// Exactly `Cell::EMPTY`, so a next-cell equal to EMPTY needs no repaint.
const ERASED: Cell = Cell::EMPTY;

impl FrameDiff {
    /// Like [`FrameDiff::compute`], but first tries to explain the damage
    /// as one vertical band shift. The result is a [`ScrolledRuns`] token:
    /// when a shift pays off, the contained runs are valid solely against
    /// a terminal that executes the shift first, and only
    /// `Presenter::emit_scrolled` can consume them; a declined detection
    /// yields a plain token, byte-for-byte the ordinary compute.
    pub fn compute_scrolled<'a>(
        &'a mut self,
        prev: &Surface,
        next: &Surface,
        damage: &[Rect],
    ) -> ScrolledRuns<'a> {
        let size = next.size();
        if size.is_empty() || prev.size() != size {
            return ScrolledRuns::plain(self.compute(prev, next, damage));
        }
        // Damage union must be a full-width band tall enough to pay.
        let bounds = Rect::from_size(size);
        let union = damage
            .iter()
            .fold(Rect::ZERO, |acc, r| acc.union(r.intersect(bounds)));
        if union.is_empty() || union.x > 0 || union.right() < size.w || union.h < MIN_BAND_H {
            return ScrolledRuns::plain(self.compute(prev, next, damage));
        }
        let Some(shift) = self.detect_shift(prev, next, union.y, union.bottom()) else {
            return ScrolledRuns::plain(self.compute(prev, next, damage));
        };

        // Scan the damage against the virtually-shifted prev.
        self.runs.clear();
        self.spans.clear();
        self.collect_spans(damage, size);
        self.spans.sort_unstable();
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
            self.scan_shifted(prev, next, &shift, y, x0, x1);
        }
        ScrolledRuns {
            shift: Some(shift),
            runs: &self.runs,
        }
    }

    /// One merged interval, compared against the post-shift terminal
    /// state of row `y`.
    fn scan_shifted(
        &mut self,
        prev: &Surface,
        next: &Surface,
        shift: &Shift,
        y: i32,
        x0: i32,
        x1: i32,
    ) {
        let src = shift.source_row(y);
        let next_row = next.row(y);
        let mut run_start: Option<i32> = None;
        for x in x0..x1 {
            let b = &next_row[x as usize];
            let equal = match src {
                RowSource::Row(sy) => {
                    let a = &prev.row(sy)[x as usize];
                    cells_equal(prev, next, a, b)
                }
                RowSource::Erased => cells_equal(prev, next, &ERASED, b),
            };
            if equal {
                if let Some(start) = run_start.take() {
                    self.push_run(y, start, x);
                }
            } else if run_start.is_none() {
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

    /// Finds the shift that makes the most rows diff-clean, if any clears
    /// the byte-win floor. Fingerprints prune; exact row comparison
    /// decides (hash collisions cost time only).
    ///
    /// The damaged band is first TRIMMED of edge rows that did not change
    /// at all: fixed chrome (headers, footers, status bars) sits inside
    /// full-frame damage on real scroll frames, and anchoring candidates
    /// on an unchanged header finds nothing. The DECSTBM region then
    /// covers exactly the moving interior, which is also byte-cheaper
    /// (the scroll never touches chrome rows).
    fn detect_shift(
        &mut self,
        prev: &Surface,
        next: &Surface,
        top: i32,
        bottom: i32,
    ) -> Option<Shift> {
        let base = top;
        self.fp_prev.clear();
        self.fp_next.clear();
        for y in top..bottom {
            self.fp_prev.push(row_fingerprint(prev, y));
            self.fp_next.push(row_fingerprint(next, y));
        }
        let fp = |v: &Vec<u64>, y: i32| v[(y - base) as usize];

        // Trim unchanged edge rows (prune by fingerprint, verify exactly).
        let mut top = top;
        let mut bottom = bottom;
        while top < bottom
            && fp(&self.fp_prev, top) == fp(&self.fp_next, top)
            && rows_equal(prev, next, top, top)
        {
            top += 1;
        }
        while bottom > top
            && fp(&self.fp_prev, bottom - 1) == fp(&self.fp_next, bottom - 1)
            && rows_equal(prev, next, bottom - 1, bottom - 1)
        {
            bottom -= 1;
        }
        let band_h = bottom - top;
        if band_h < MIN_BAND_H {
            return None;
        }

        // Candidate shifts: alignments of the trimmed band's FIRST next
        // row inside prev (up anchors) and LAST next row (down anchors).
        let mut candidates = [(0i32, false); MAX_CANDIDATES * 2];
        let mut n_cand = 0;
        for n in 1..band_h - 1 {
            if n_cand < MAX_CANDIDATES && fp(&self.fp_next, top) == fp(&self.fp_prev, top + n) {
                candidates[n_cand] = (n, true);
                n_cand += 1;
            }
        }
        let mut down_found = 0;
        for n in 1..band_h - 1 {
            if down_found < MAX_CANDIDATES
                && fp(&self.fp_next, bottom - 1) == fp(&self.fp_prev, bottom - 1 - n)
            {
                candidates[n_cand] = (n, false);
                n_cand += 1;
                down_found += 1;
            }
        }

        let mut best: Option<(Shift, i32)> = None;
        for &(n, up) in &candidates[..n_cand] {
            let shift = Shift { top, bottom, n, up };
            let mut saved = 0;
            for y in top..bottom {
                if let RowSource::Row(sy) = shift.source_row(y) {
                    if sy != y
                        && fp(&self.fp_next, y) == fp(&self.fp_prev, sy)
                        && rows_equal(prev, next, sy, y)
                    {
                        saved += 1;
                    }
                }
            }
            if saved >= MIN_SAVED_ROWS && best.is_none_or(|(_, s)| saved > s) {
                best = Some((shift, saved));
            }
        }
        best.map(|(s, _)| s)
    }
}

impl Shift {
    /// Where the terminal reads row `y`'s content from after this shift:
    /// a prev row, or BCE-erased blank for entering rows. Rows outside
    /// the band are untouched by the scroll.
    fn source_row(&self, y: i32) -> RowSource {
        if y < self.top || y >= self.bottom {
            return RowSource::Row(y);
        }
        if self.up {
            let sy = y + self.n;
            if sy < self.bottom {
                RowSource::Row(sy)
            } else {
                RowSource::Erased
            }
        } else {
            let sy = y - self.n;
            if sy >= self.top {
                RowSource::Row(sy)
            } else {
                RowSource::Erased
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum RowSource {
    Row(i32),
    Erased,
}

fn rows_equal(prev: &Surface, next: &Surface, prev_y: i32, next_y: i32) -> bool {
    let pr = prev.row(prev_y);
    let nr = next.row(next_y);
    pr.len() == nr.len()
        && pr
            .iter()
            .zip(nr)
            .all(|(a, b)| cells_equal(prev, next, a, b))
}

/// FNV-1a over the row's rendered content: glyph text (pool-resolved),
/// colors, attrs, link URI. A prune only — equality is always re-verified
/// exactly, so collisions are a performance non-event.
fn row_fingerprint(s: &Surface, y: i32) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x100_0000_01b3;
    let mut h = OFFSET;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(PRIME);
        }
    };
    for cell in s.row(y) {
        eat(s.glyph_str(cell).as_bytes());
        eat(&[
            cell.fg.r, cell.fg.g, cell.fg.b, cell.fg.a, cell.bg.r, cell.bg.g, cell.bg.b, cell.bg.a,
            cell.ul.r, cell.ul.g, cell.ul.b, cell.ul.a,
        ]);
        eat(&cell.attrs.bits().to_le_bytes());
        if cell.link != 0 {
            eat(s.link_uri(cell.link).unwrap_or("").as_bytes());
        }
        // Cell separator so ("ab","c") never hashes like ("a","bc").
        eat(&[0xFF]);
    }
    h
}

#[cfg(test)]
#[path = "scroll_tests.rs"]
mod tests;
