//! The VT model's cell grid: contents, styles, wide-glyph pairing rules,
//! clears and scrolling. Pure data — the escape parser lives in `vt.rs`.
//!
//! OWNER: REDTEAM.
//!
//! The one invariant this file exists to police: a wide glyph is ALWAYS a
//! `Text` leader immediately followed by exactly one `Continuation`, both
//! carrying the same paint. Every write path repairs neighbours so a torn
//! pair can never survive; renderer bytes that would tear a pair on a real
//! terminal therefore change visible cells here too, and the diff/present
//! property test catches it.

use crate::base::Rgba;
use unicode_width::UnicodeWidthStr;

/// SGR attribute bits tracked by the model. The set mirrors what the
/// render contract declares the presenter may emit (render.md §2.4),
/// including blink/hidden and undercurl (SGR 4:3).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Attrs(pub u16);

impl Attrs {
    pub const BOLD: u16 = 1 << 0;
    pub const DIM: u16 = 1 << 1;
    pub const ITALIC: u16 = 1 << 2;
    pub const UNDERLINE: u16 = 1 << 3;
    pub const REVERSE: u16 = 1 << 4;
    pub const STRIKE: u16 = 1 << 5;
    pub const BLINK: u16 = 1 << 6;
    pub const HIDDEN: u16 = 1 << 7;
    pub const UNDERCURL: u16 = 1 << 8;

    pub const fn contains(self, bit: u16) -> bool {
        self.0 & bit != 0
    }

    pub fn set(&mut self, bit: u16, on: bool) {
        if on {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Stable one-letter-per-attribute label, e.g. `BU` for bold+underline
    /// (K = blink, H = hidden, W = undercurl/wavy).
    pub fn label(self) -> String {
        let mut s = String::new();
        for (bit, ch) in [
            (Self::BOLD, 'B'),
            (Self::DIM, 'D'),
            (Self::ITALIC, 'I'),
            (Self::UNDERLINE, 'U'),
            (Self::REVERSE, 'R'),
            (Self::STRIKE, 'S'),
            (Self::BLINK, 'K'),
            (Self::HIDDEN, 'H'),
            (Self::UNDERCURL, 'W'),
        ] {
            if self.contains(bit) {
                s.push(ch);
            }
        }
        s
    }
}

/// Colors + attributes + hyperlink applied to written cells. `None` colors
/// mean "terminal default" — a distinct state from any concrete Rgba, so
/// presenter downlevel tests can assert SGR 39/49 exactly. `ul` is the
/// underline color (SGR 58/59; `None` = follow fg).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Paint {
    pub fg: Option<Rgba>,
    pub bg: Option<Rgba>,
    pub ul: Option<Rgba>,
    pub attrs: Attrs,
    pub link: Option<u32>,
}

impl Paint {
    pub fn is_default(self) -> bool {
        self == Paint::default()
    }

    /// The paint an erase operation (ED/EL/ECH, scroll fill) leaves behind:
    /// background color erase (BCE) keeps the current bg, resets the rest.
    /// This matches xterm and every modern emulator we target.
    pub fn erase_paint(self) -> Paint {
        Paint {
            bg: self.bg,
            ..Paint::default()
        }
    }
}

/// What a cell holds. `Blank` (erased) and `Text(" ")` (a printed space)
/// render identically but are kept distinct so tests can tell "cleared"
/// from "overwritten" when hunting presenter bugs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CellContent {
    #[default]
    Blank,
    /// A printed grapheme: one base char plus any combining chars appended.
    Text(String),
    /// Right half of a wide glyph; the leader is the cell to the left.
    Continuation,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VtCell {
    pub content: CellContent,
    pub paint: Paint,
}

impl VtCell {
    /// Display char for plain-text dumps: leaders show their base char,
    /// blanks show a space, continuations are skipped by callers.
    pub fn display(&self) -> &str {
        match &self.content {
            CellContent::Blank => " ",
            CellContent::Text(s) => s,
            CellContent::Continuation => "",
        }
    }

    /// First char of the content (' ' for blank, '\0' for continuation) —
    /// the convenient form for cell-level assertions in tests.
    pub fn ch(&self) -> char {
        match &self.content {
            CellContent::Blank => ' ',
            CellContent::Text(s) => s.chars().next().unwrap_or(' '),
            CellContent::Continuation => '\0',
        }
    }

    pub fn is_wide_leader(&self) -> bool {
        match &self.content {
            // Whole-cluster width, not first-char width: VS16 emoji
            // presentation ("☁" + U+FE0F) widens a narrow base char, and
            // unicode-width's str metric accounts for that.
            CellContent::Text(s) => s.width() >= 2,
            _ => false,
        }
    }

    pub fn is_continuation(&self) -> bool {
        matches!(self.content, CellContent::Continuation)
    }
}

/// Fixed-size cell grid with wide-pair repair, BCE clears and scrolling.
pub struct Grid {
    pub w: i32,
    pub h: i32,
    rows: Vec<Vec<VtCell>>,
}

impl Grid {
    pub fn new(w: i32, h: i32) -> Grid {
        let w = w.max(1);
        let h = h.max(1);
        Grid {
            w,
            h,
            rows: (0..h)
                .map(|_| (0..w).map(|_| VtCell::default()).collect())
                .collect(),
        }
    }

    pub fn cell(&self, x: i32, y: i32) -> Option<&VtCell> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        Some(&self.rows[y as usize][x as usize])
    }

    fn cell_mut(&mut self, x: i32, y: i32) -> Option<&mut VtCell> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        Some(&mut self.rows[y as usize][x as usize])
    }

    /// Blank a single cell keeping its paint (the repair primitive: a torn
    /// wide pair leaves a styled blank, like real terminals do).
    fn blank_content(&mut self, x: i32, y: i32) {
        if let Some(c) = self.cell_mut(x, y) {
            c.content = CellContent::Blank;
        }
    }

    /// If writing into (x, y) would tear a wide pair, blank the orphaned
    /// half first. Called for every cell a write is about to occupy.
    fn repair_for_write(&mut self, x: i32, y: i32) {
        let (is_cont, is_leader) = match self.cell(x, y) {
            Some(c) => (c.is_continuation(), c.is_wide_leader()),
            None => return,
        };
        if is_cont {
            self.blank_content(x - 1, y);
        }
        if is_leader {
            self.blank_content(x + 1, y);
        }
    }

    /// Overwrite a cell's content with a single char, REUSING the
    /// existing String's buffer when the cell already held text — the
    /// referee repaints every cell every frame in property tests, and a
    /// fresh String per print made the model slower than the engine it
    /// judges (cycle-2 perf run).
    fn set_char_reusing(&mut self, x: i32, y: i32, ch: char, paint: Paint) {
        if let Some(c) = self.cell_mut(x, y) {
            match &mut c.content {
                CellContent::Text(s) => {
                    s.clear();
                    s.push(ch);
                }
                _ => c.content = CellContent::Text(ch.to_string()),
            }
            c.paint = paint;
        }
    }

    /// Write a narrow (width 1) char.
    pub fn put_narrow(&mut self, x: i32, y: i32, ch: char, paint: Paint) {
        if self.cell(x, y).is_none() {
            return;
        }
        self.repair_for_write(x, y);
        self.set_char_reusing(x, y, ch, paint);
    }

    /// Write a wide (width 2) char: leader at x, continuation at x+1.
    /// Caller guarantees x+1 < w (the VT layer owns margin policy).
    pub fn put_wide(&mut self, x: i32, y: i32, ch: char, paint: Paint) {
        if x + 1 >= self.w || self.cell(x, y).is_none() {
            return;
        }
        self.repair_for_write(x, y);
        self.repair_for_write(x + 1, y);
        self.set_char_reusing(x, y, ch, paint);
        if let Some(c) = self.cell_mut(x + 1, y) {
            c.content = CellContent::Continuation;
            c.paint = paint;
        }
    }

    /// Ensure a text cell whose cluster now measures wide (e.g. after a
    /// VS16 append) owns a continuation cell. Call AFTER mutating the
    /// cluster. Returns true if a continuation was placed. At the last
    /// column there is no room: the glyph blanks (matching `put_wide`'s
    /// margin policy — never half a glyph).
    pub fn widen_to_wide(&mut self, x: i32, y: i32) -> bool {
        let needs = self.cell(x, y).map(|c| c.is_wide_leader()).unwrap_or(false)
            && !self
                .cell(x + 1, y)
                .map(|c| c.is_continuation())
                .unwrap_or(false);
        if !needs {
            return false;
        }
        if x + 1 >= self.w {
            self.blank_content(x, y);
            return false;
        }
        self.repair_for_write(x + 1, y);
        let paint = self.cell(x, y).map(|c| c.paint).unwrap_or_default();
        if let Some(c) = self.cell_mut(x + 1, y) {
            c.content = CellContent::Continuation;
            c.paint = paint;
        }
        true
    }

    /// Append a combining char to the glyph at (x, y) (leader-adjusted:
    /// pass the cell the base char lives in). Returns false if there is
    /// no text there to combine with.
    pub fn append_combining(&mut self, x: i32, y: i32, ch: char) -> bool {
        // Combining with a continuation means combining with its leader.
        let target_x = match self.cell(x, y) {
            Some(c) if c.is_continuation() => x - 1,
            Some(_) => x,
            None => return false,
        };
        match self.cell_mut(target_x, y) {
            Some(VtCell {
                content: CellContent::Text(s),
                ..
            }) => {
                s.push(ch);
                true
            }
            _ => false,
        }
    }

    /// Erase cells [x0, x1) on row y with BCE paint, repairing torn pairs
    /// at both boundaries of the range.
    pub fn erase_row_range(&mut self, y: i32, x0: i32, x1: i32, erase: Paint) {
        let x0 = x0.clamp(0, self.w);
        let x1 = x1.clamp(0, self.w);
        if y < 0 || y >= self.h || x0 >= x1 {
            return;
        }
        // Boundary repair BEFORE the clear: an erased continuation orphans
        // a leader to the left of the range; an erased leader orphans a
        // continuation to the right of it.
        if self
            .cell(x0, y)
            .map(|c| c.is_continuation())
            .unwrap_or(false)
        {
            self.blank_content(x0 - 1, y);
        }
        if self
            .cell(x1 - 1, y)
            .map(|c| c.is_wide_leader())
            .unwrap_or(false)
        {
            self.blank_content(x1, y);
        }
        for x in x0..x1 {
            if let Some(c) = self.cell_mut(x, y) {
                c.content = CellContent::Blank;
                c.paint = erase;
            }
        }
    }

    fn blank_row(&self, erase: Paint) -> Vec<VtCell> {
        (0..self.w)
            .map(|_| VtCell {
                content: CellContent::Blank,
                paint: erase,
            })
            .collect()
    }

    /// Scroll the whole grid up by n rows, filling the bottom with erased
    /// rows (BCE paint).
    pub fn scroll_up(&mut self, n: i32, erase: Paint) {
        self.scroll_up_region(0, self.h - 1, n, erase);
    }

    /// Scroll down (for RI at the top row), filling the top with erased rows.
    pub fn scroll_down(&mut self, n: i32, erase: Paint) {
        self.scroll_down_region(0, self.h - 1, n, erase);
    }

    /// Scroll rows [top..=bottom] up by n; the vacated bottom rows are
    /// erased (BCE). Rows are moved wholesale so wide pairs stay intact.
    pub fn scroll_up_region(&mut self, top: i32, bottom: i32, n: i32, erase: Paint) {
        let top = top.clamp(0, self.h - 1) as usize;
        let bottom = bottom.clamp(0, self.h - 1) as usize;
        if bottom < top {
            return;
        }
        let span = bottom - top + 1;
        let n = (n.max(0) as usize).min(span);
        if n == 0 {
            return;
        }
        let blank = self.blank_row(erase);
        self.rows[top..=bottom].rotate_left(n);
        for row in &mut self.rows[bottom + 1 - n..=bottom] {
            *row = blank.clone();
        }
    }

    /// Scroll rows [top..=bottom] down by n; the vacated top rows are
    /// erased (BCE).
    pub fn scroll_down_region(&mut self, top: i32, bottom: i32, n: i32, erase: Paint) {
        let top = top.clamp(0, self.h - 1) as usize;
        let bottom = bottom.clamp(0, self.h - 1) as usize;
        if bottom < top {
            return;
        }
        let span = bottom - top + 1;
        let n = (n.max(0) as usize).min(span);
        if n == 0 {
            return;
        }
        self.rows[top..=bottom].rotate_right(n);
        let blank = self.blank_row(erase);
        for row in &mut self.rows[top..top + n] {
            *row = blank.clone();
        }
    }

    /// IL: insert n blank lines at `at`, pushing rows down within
    /// [at..=bottom]; rows shifted past `bottom` are lost.
    pub fn insert_lines(&mut self, at: i32, bottom: i32, n: i32, erase: Paint) {
        self.scroll_down_region(at, bottom, n, erase);
    }

    /// DL: delete n lines at `at`, pulling rows up within [at..=bottom];
    /// vacated bottom rows are erased.
    pub fn delete_lines(&mut self, at: i32, bottom: i32, n: i32, erase: Paint) {
        self.scroll_up_region(at, bottom, n, erase);
    }

    /// Reset every cell to the default blank.
    pub fn clear_all(&mut self, erase: Paint) {
        for y in 0..self.h {
            self.erase_row_range(y, 0, self.w, erase);
        }
    }

    /// Plain-text view of one row (continuations skipped so wide glyphs
    /// occupy their two columns naturally when printed).
    pub fn row_text(&self, y: i32) -> String {
        let mut s = String::new();
        for x in 0..self.w {
            if let Some(c) = self.cell(x, y) {
                s.push_str(c.display());
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> Paint {
        Paint {
            fg: Some(Rgba::rgb(255, 0, 0)),
            ..Paint::default()
        }
    }

    #[test]
    fn wide_pair_repair_on_overwrite() {
        let mut g = Grid::new(6, 1);
        g.put_wide(1, 0, '世', red());
        assert!(g.cell(1, 0).unwrap().is_wide_leader());
        assert!(g.cell(2, 0).unwrap().is_continuation());
        // Overwrite the continuation: leader must blank.
        g.put_narrow(2, 0, 'x', Paint::default());
        assert_eq!(g.cell(1, 0).unwrap().content, CellContent::Blank);
        assert_eq!(g.cell(2, 0).unwrap().ch(), 'x');
    }

    #[test]
    fn erase_range_repairs_boundaries() {
        let mut g = Grid::new(6, 1);
        g.put_wide(0, 0, '你', red());
        g.put_wide(2, 0, '好', red());
        // Erase [1,3): cuts 你's continuation and 好's leader.
        g.erase_row_range(0, 1, 3, Paint::default());
        assert_eq!(g.cell(0, 0).unwrap().content, CellContent::Blank);
        assert_eq!(g.cell(3, 0).unwrap().content, CellContent::Blank);
    }

    #[test]
    fn combining_appends_to_leader() {
        let mut g = Grid::new(4, 1);
        g.put_narrow(0, 0, 'e', Paint::default());
        assert!(g.append_combining(0, 0, '\u{0301}'));
        assert_eq!(g.cell(0, 0).unwrap().display(), "e\u{0301}");
        assert!(!g.append_combining(3, 0, '\u{0301}'));
    }
}
