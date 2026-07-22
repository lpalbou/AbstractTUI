//! TextArea editing model (private sibling of textarea.rs — file-size
//! split, same pattern as feed_typeset.rs): the byte-tiling soft-wrap
//! row map, caret/selection ops over grapheme clusters, and the
//! history recall state machine (backlog 0120).
//!
//! ## Why not `text::wrap`
//!
//! The renderer's wrapper CONSUMES whitespace at break points — right
//! for display, wrong for an editor, where every byte the user can
//! reach must have exactly one home. [`RowMap`] therefore TILES the
//! text: every byte belongs to exactly one row (invariant-tested), soft
//! breaks prefer the boundary after a whitespace run, and whitespace
//! that overflows the width hangs at the row edge (clipped by the
//! renderer) instead of vanishing. Widths come from `text::segments` —
//! the same authority the renderer uses — so columns and pixels agree.
//!
//! OWNER: REACT.

use crate::ui::{Key, Mods};
use crate::widgets::input::{word_step, ClusterMap};

/// Enter-key policy (builder-owned; backlog 0120 §3). Alt+Enter always
/// inserts a newline (works on every wire); Shift+Enter also inserts
/// where the kitty keyboard protocol reports it — legacy terminals
/// cannot carry that chord, and the policy never advertises it as the
/// only path (docs/faq.md:164).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SubmitPolicy {
    /// Plain Enter submits; Alt+Enter, Ctrl+J (every wire) and kitty
    /// Shift+Enter insert.
    #[default]
    EnterSubmits,
    /// Plain Enter inserts a newline; submission is the app's own
    /// affordance (a button, a shortcut).
    EnterInserts,
}

/// Caret + selection + viewport state. Byte offsets are ALWAYS grapheme
/// cluster boundaries (the RT3-2 contract inherited from `TextInput`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Caret {
    /// Caret byte offset (cluster boundary, 0..=text.len()).
    pub byte: usize,
    /// Selection anchor byte; None = no selection.
    pub anchor: Option<usize>,
    /// Remembered goal column for vertical movement runs.
    pub goal: Option<i32>,
    /// Soft-boundary affinity: a caret exactly at a soft-wrap boundary
    /// renders at the END of the previous row (End key) instead of the
    /// start of the next.
    pub sticky: bool,
    /// First visible visual row (internal scroll once past max_rows).
    pub top: i32,
}

impl Caret {
    pub fn origin() -> Caret {
        Caret {
            byte: 0,
            anchor: None,
            goal: None,
            sticky: false,
            top: 0,
        }
    }
}

/// One visual row: byte range within the whole text. `end` includes a
/// trailing newline cluster when the row is a hard line end;
/// `text_end` is where RENDERED content stops (excludes the newline).
/// Soft-wrapped rows have `text_end == end`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    pub start: usize,
    pub text_end: usize,
    pub end: usize,
}

/// Byte-tiling soft wrap of the whole text at a fixed width. Rows tile
/// `0..text.len()` exactly; an empty text still yields one row (the
/// caret needs a home).
pub(crate) struct RowMap {
    pub rows: Vec<Row>,
}

impl RowMap {
    pub fn build(text: &str, width: i32) -> RowMap {
        let width = width.max(1);
        let mut rows: Vec<Row> = Vec::new();
        let mut start = 0usize; // current row start byte
        let mut col = 0i32; // accumulated row width
                            // Break candidate: byte position AFTER the latest whitespace
                            // cluster + the accumulated width at that position.
        let mut break_at: Option<(usize, i32)> = None;
        for seg in crate::text::segments(text) {
            let seg_end = seg.offset + seg.cluster.len();
            let first = seg.cluster.chars().next();
            let is_ws = first.is_some_and(char::is_whitespace);
            if crate::text::is_control_cluster(seg.cluster) {
                if seg.cluster.contains('\n') {
                    // Hard line end: the newline cluster rides the row it
                    // terminates; the next row starts after it.
                    rows.push(Row {
                        start,
                        text_end: seg.offset,
                        end: seg_end,
                    });
                    start = seg_end;
                    col = 0;
                    break_at = None;
                } else if is_ws {
                    // Width-0 whitespace (tab): rides along, breakable after.
                    break_at = Some((seg_end, col));
                }
                continue;
            }
            // Overflow: close the row at the best break point — but only
            // for VISIBLE non-whitespace clusters. Overflowing whitespace
            // hangs at the row edge (clipped), so wrapped output shows
            // "hello world" / "next", never a leading space (visual
            // parity with text::wrap without losing the bytes).
            if !is_ws && col + seg.width > width && col > 0 {
                match break_at {
                    Some((b, bcol)) if b > start && b <= seg.offset => {
                        rows.push(Row {
                            start,
                            text_end: b,
                            end: b,
                        });
                        start = b;
                        col -= bcol;
                    }
                    _ => {
                        // No usable word break: cut mid-word at the
                        // cluster boundary (never inside a cluster).
                        rows.push(Row {
                            start,
                            text_end: seg.offset,
                            end: seg.offset,
                        });
                        start = seg.offset;
                        col = 0;
                    }
                }
                break_at = None;
            }
            col += seg.width;
            if is_ws {
                break_at = Some((seg_end, col));
            }
        }
        rows.push(Row {
            start,
            text_end: text.len(),
            end: text.len(),
        });
        // A completely full last row leaves the caret-at-end no cell to
        // live in (columns are 0..width-1) — give it a phantom empty
        // row, exactly like editors wrapping the cursor to a fresh
        // line. Tiling holds: the phantom is zero-length at text.len().
        if col >= width {
            rows.push(Row {
                start: text.len(),
                text_end: text.len(),
                end: text.len(),
            });
        }
        RowMap { rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Visual row holding `byte`. A byte exactly at a boundary belongs
    /// to the row STARTING there; `sticky` flips a SOFT boundary to the
    /// previous row (End-key affinity — a hard newline is never sticky:
    /// the caret after it genuinely lives on the next row).
    pub fn row_of(&self, byte: usize, sticky: bool) -> usize {
        let idx = self
            .rows
            .partition_point(|r| r.end <= byte)
            .min(self.rows.len() - 1);
        if sticky && idx > 0 {
            let prev = self.rows[idx - 1];
            if prev.end == byte && prev.text_end == prev.end {
                return idx - 1;
            }
        }
        idx
    }

    /// Display column of `byte` within its row (0-based).
    pub fn col_of(&self, text: &str, byte: usize, row_idx: usize) -> i32 {
        let row = self.rows[row_idx];
        let slice = &text[row.start..row.text_end];
        let rel = byte.saturating_sub(row.start).min(slice.len());
        crate::text::width(&slice[..rel])
    }

    /// Caret byte closest to `goal` columns into row `row_idx` (snaps to
    /// cluster starts; past the row's content lands at `text_end`).
    pub fn byte_at_col(&self, text: &str, row_idx: usize, goal: i32) -> usize {
        let row = self.rows[row_idx];
        let slice = &text[row.start..row.text_end];
        let mut col = 0i32;
        for seg in crate::text::segments(slice) {
            if seg.width > 0 && col + seg.width > goal {
                return row.start + seg.offset;
            }
            col += seg.width;
        }
        row.text_end
    }

    /// (row, col) of the caret, honoring soft-boundary affinity.
    pub fn visual(&self, text: &str, byte: usize, sticky: bool) -> (usize, i32) {
        let row = self.row_of(byte, sticky);
        (row, self.col_of(text, byte, row))
    }
}

/// Snap `byte` to a cluster boundary: boundaries stay put, mid-cluster
/// positions snap to the enclosing cluster's END (the `cluster_after`
/// convention — an inserted ZWJ/combining scalar that merged clusters
/// leaves the caret after the merged whole).
pub(crate) fn snap_boundary(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    for seg in crate::text::segments(text) {
        if seg.offset == byte {
            return byte;
        }
        let end = seg.offset + seg.cluster.len();
        if end > byte {
            return end;
        }
    }
    text.len()
}

/// Selection byte range (lo, hi); (usize::MAX, usize::MAX) when empty.
pub(crate) fn selection_range(c: &Caret) -> (usize, usize) {
    match c.anchor {
        Some(a) if a != c.byte => (a.min(c.byte), a.max(c.byte)),
        _ => (usize::MAX, usize::MAX),
    }
}

/// Remove the selected range, if any. Caret lands at the cut.
pub(crate) fn delete_selection(text: &mut String, c: &mut Caret) -> bool {
    let (lo, hi) = selection_range(c);
    if lo == usize::MAX {
        return false;
    }
    text.replace_range(lo..hi, "");
    c.byte = lo;
    c.anchor = None;
    true
}

/// Insert `s` at the caret (replacing any selection). Multiline-safe:
/// newlines in `s` are ordinary content (the block-paste path).
pub(crate) fn insert_at_caret(text: &mut String, c: &mut Caret, s: &str) {
    delete_selection(text, c);
    let at = snap_boundary(text, c.byte.min(text.len()));
    text.insert_str(at, s);
    c.byte = snap_boundary(text, at + s.len());
    c.anchor = None;
    c.goal = None;
    c.sticky = false;
}

/// What a key did to the buffer — the caller (textarea.rs) maps these
/// onto signals, callbacks and the history store.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EditOutcome {
    /// Consumed; `edited` = buffer content changed (fire on_change).
    Handled { edited: bool },
    /// Plain Enter under `SubmitPolicy::EnterSubmits`.
    Submit,
    /// Up at the very start of the buffer (or an empty buffer): the
    /// history edge (backlog 0120 §4, `arrow_nav_action` semantics).
    HistoryBack,
    /// Down at the very end: mirror edge.
    HistoryForward,
    /// Not ours (Tab traversal, Escape, unbound chords fall through).
    Ignored,
}

/// Apply one key to the buffer + caret. Pure over its arguments; the
/// decision table is pinned by textarea_tests.rs.
pub(crate) fn apply_key(
    text: &mut String,
    c: &mut Caret,
    key: Key,
    mods: Mods,
    width: i32,
    policy: SubmitPolicy,
) -> EditOutcome {
    let shift = mods.contains(Mods::SHIFT);
    let alt = mods.contains(Mods::ALT);
    let ctrl = mods.contains(Mods::CTRL);
    // Defensive clamp against external set_text: stale offsets snap in.
    c.byte = snap_boundary(text, c.byte.min(text.len()));
    if let Some(a) = c.anchor {
        c.anchor = Some(snap_boundary(text, a.min(text.len())));
    }

    // Anchor bookkeeping shared by every motion.
    let move_to = |c: &mut Caret, target: usize, sticky: bool| {
        if shift {
            if c.anchor.is_none() {
                c.anchor = Some(c.byte);
            }
        } else {
            c.anchor = None;
        }
        c.byte = target;
        c.sticky = sticky;
    };

    match key {
        // ---- horizontal motion (clears the vertical goal) -------------
        Key::Left => {
            let target = if alt {
                word_step_bytes(text, c.byte, -1)
            } else {
                crate::text::prev_boundary(text, c.byte)
            };
            move_to(c, target, false);
            c.goal = None;
            EditOutcome::Handled { edited: false }
        }
        Key::Right => {
            let target = if alt {
                word_step_bytes(text, c.byte, 1)
            } else {
                crate::text::next_boundary(text, c.byte)
            };
            move_to(c, target, false);
            c.goal = None;
            EditOutcome::Handled { edited: false }
        }
        Key::Home => {
            let target = if ctrl {
                0
            } else {
                let rows = RowMap::build(text, width);
                rows.rows[rows.row_of(c.byte, c.sticky)].start
            };
            move_to(c, target, false);
            c.goal = None;
            EditOutcome::Handled { edited: false }
        }
        Key::End => {
            let (target, sticky) = if ctrl {
                (text.len(), false)
            } else {
                let rows = RowMap::build(text, width);
                let row = rows.rows[rows.row_of(c.byte, c.sticky)];
                // Soft rows with SPARE width: land ON the boundary with
                // end affinity (the caret renders in this row's empty
                // tail). Full rows have no cell at the margin — the
                // boundary byte renders at the next row's start.
                (row.text_end, row_end_has_room(&rows, text, row, width))
            };
            move_to(c, target, sticky);
            c.goal = None;
            EditOutcome::Handled { edited: false }
        }

        // ---- vertical motion (goal column; history at the edges) ------
        Key::Up | Key::Down => {
            let rows = RowMap::build(text, width);
            let (row, col) = rows.visual(text, c.byte, c.sticky);
            let goal = c.goal.unwrap_or(col);
            if key == Key::Up {
                if row > 0 {
                    let target = rows.byte_at_col(text, row - 1, goal);
                    let sticky = soft_row_end(&rows, text, row - 1, target, width);
                    move_to(c, target, sticky);
                    c.goal = Some(goal);
                } else if shift || c.byte > 0 {
                    // Buffer first: jump to the text start (with Shift
                    // this is a selection extension, never history).
                    move_to(c, 0, false);
                    c.goal = None;
                } else {
                    // ...and only the edge reaches for history.
                    return EditOutcome::HistoryBack;
                }
            } else {
                let last = rows.len() - 1;
                if row < last {
                    let target = rows.byte_at_col(text, row + 1, goal);
                    let sticky = soft_row_end(&rows, text, row + 1, target, width);
                    move_to(c, target, sticky);
                    c.goal = Some(goal);
                } else if shift || c.byte < text.len() {
                    move_to(c, text.len(), false);
                    c.goal = None;
                } else {
                    return EditOutcome::HistoryForward;
                }
            }
            EditOutcome::Handled { edited: false }
        }

        // ---- edits (cluster-atomic byte ranges) ------------------------
        Key::Char(ch) if !ctrl && !alt => {
            let mut buf = [0u8; 4];
            insert_at_caret(text, c, ch.encode_utf8(&mut buf));
            EditOutcome::Handled { edited: true }
        }
        Key::Backspace => {
            if !delete_selection(text, c) && c.byte > 0 {
                let cut = crate::text::prev_boundary(text, c.byte);
                text.replace_range(cut..c.byte, "");
                c.byte = cut;
            }
            c.goal = None;
            c.sticky = false;
            EditOutcome::Handled { edited: true }
        }
        Key::Delete => {
            if !delete_selection(text, c) && c.byte < text.len() {
                let end = crate::text::next_boundary(text, c.byte);
                text.replace_range(c.byte..end, "");
            }
            c.goal = None;
            c.sticky = false;
            EditOutcome::Handled { edited: true }
        }

        // ---- Enter: submit vs newline (backlog 0120 §3) ----------------
        Key::Enter => {
            if alt || shift {
                insert_at_caret(text, c, "\n");
                return EditOutcome::Handled { edited: true };
            }
            match policy {
                SubmitPolicy::EnterSubmits => EditOutcome::Submit,
                SubmitPolicy::EnterInserts => {
                    insert_at_caret(text, c, "\n");
                    EditOutcome::Handled { edited: true }
                }
            }
        }
        // Ctrl+J: the UNIVERSAL newline chord (backlog 0295). A legacy
        // terminal's 0x0a byte IS Ctrl+J (`input::legacy::control_key`)
        // and the kitty protocol reports the same identity, so this one
        // arm gives every terminal a working newline even where
        // Shift+Enter cannot be reported. Built in so apps stop
        // hand-rolling the fallback (and their hint text stays true).
        Key::Char('j') if ctrl && !alt => {
            insert_at_caret(text, c, "\n");
            EditOutcome::Handled { edited: true }
        }

        _ => EditOutcome::Ignored,
    }
}

/// Did `byte` land exactly on the soft end boundary of `row`, with a
/// spare cell to render in? (Vertical moves onto such a row must render
/// on THAT row, not the next.)
fn soft_row_end(rows: &RowMap, text: &str, row: usize, byte: usize, width: i32) -> bool {
    let r = rows.rows[row];
    byte == r.end && row_end_has_room(rows, text, r, width)
}

/// End affinity is only honest when the row's rendered content leaves a
/// spare column for the caret block (columns are 0..width-1; a full row
/// has no margin cell — its boundary byte lives on the next row).
fn row_end_has_room(_rows: &RowMap, text: &str, row: Row, width: i32) -> bool {
    row.text_end == row.end
        && row.end > row.start
        && crate::text::width(&text[row.start..row.text_end]) < width
}

/// Word jump over the WHOLE document in byte space (newline clusters are
/// separators, so jumps cross lines naturally). Reuses `TextInput`'s
/// cluster-index `word_step` through one whole-text map.
fn word_step_bytes(text: &str, from: usize, dir: i32) -> usize {
    let map = ClusterMap::of(text);
    let idx = map.cluster_after(from);
    let target = word_step(text, &map, idx, dir);
    map.byte(target)
}

/// Keep the caret's visual row inside the `visible`-row window.
pub(crate) fn adjust_top(c: &mut Caret, caret_row: i32, total_rows: i32, visible: i32) {
    let visible = visible.max(1);
    if caret_row < c.top {
        c.top = caret_row;
    }
    if caret_row >= c.top + visible {
        c.top = caret_row - visible + 1;
    }
    c.top = c.top.clamp(0, (total_rows - visible).max(0));
}

// ---------------------------------------------------------------------
// History recall (backlog 0120 §4): edge-triggered, draft-preserving.
// ---------------------------------------------------------------------

/// Submission history with shell-style recall. The DRAFT (the buffer as
/// it stood when navigation started) is saved on the first step back
/// and restored when stepping past the newest entry; edits made to a
/// RECALLED entry are ephemeral (lost on further navigation) — the
/// draft is the protected thing, matching the reference console.
pub(crate) struct History {
    entries: Vec<String>,
    nav: Option<usize>,
    draft: Option<String>,
    cap: usize,
}

impl History {
    pub fn new(cap: usize) -> History {
        History {
            entries: Vec::new(),
            nav: None,
            draft: None,
            cap: cap.max(1),
        }
    }

    /// Record a submitted entry. Empty entries and consecutive
    /// duplicates are skipped; any recall-in-progress ends (the next Up
    /// starts from the newest entry again).
    pub fn push(&mut self, entry: &str) {
        self.nav = None;
        self.draft = None;
        if entry.is_empty() || self.entries.last().is_some_and(|e| e == entry) {
            return;
        }
        self.entries.push(entry.to_string());
        if self.entries.len() > self.cap {
            let overflow = self.entries.len() - self.cap;
            self.entries.drain(..overflow);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Step to an older entry. The first step saves `current` as the
    /// draft. None = nothing older (empty history, or already at the
    /// oldest — the buffer stays put).
    pub fn back(&mut self, current: &str) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        match self.nav {
            None => {
                self.draft = Some(current.to_string());
                self.nav = Some(self.entries.len() - 1);
            }
            Some(0) => return None,
            Some(i) => self.nav = Some(i - 1),
        }
        self.nav.map(|i| self.entries[i].clone())
    }

    /// Step to a newer entry; past the newest restores the saved draft
    /// and ends navigation. None = not navigating.
    pub fn forward(&mut self) -> Option<String> {
        let i = self.nav?;
        if i + 1 < self.entries.len() {
            self.nav = Some(i + 1);
            Some(self.entries[i + 1].clone())
        } else {
            self.nav = None;
            Some(self.draft.take().unwrap_or_default())
        }
    }
}
