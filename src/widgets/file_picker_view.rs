//! FilePicker's pure view half (backlog first-app/0273): path shaping,
//! filtering, size formatting, and the rows/breadcrumb painters. Split
//! `#[path]` sibling of file_picker.rs for the file budget; no signals
//! here — the widget snapshots state and hands plain data in.
//!
//! OWNER: REACT.

use std::rc::Rc;

use crate::base::{Point, Rect, Rgba};
use crate::render::Style;
use crate::ui::StyledCanvas;

use super::source::FileEntry;

/// Resolved token inks the painters consume (RT1-9b: resolved at
/// view-build time, plain `Rgba` in draw closures).
#[derive(Copy, Clone)]
pub(crate) struct PickerPalette {
    pub text: Rgba,
    pub muted: Rgba,
    pub faint: Rgba,
    pub accent: Rgba,
    pub error: Rgba,
    pub sel_bg: Rgba,
    pub sel_fg: Rgba,
    pub ground: Rgba,
    pub track: Rgba,
    pub thumb: Rgba,
    pub mark: Rgba,
}

/// One render snapshot of the list area, built from signals inside the
/// dyn_view and moved into the draw closure.
pub(crate) enum RowsContent {
    /// The source refused the directory — rendered honestly.
    Error(String),
    /// The directory listed fine and has no entries.
    Empty,
    /// Entries exist but the filter excluded all of them.
    NoMatches,
    /// Visible rows: filtered indices into `entries`, selection index
    /// INTO `filtered`, first visible row, and a per-filtered-row
    /// marked flag.
    Rows {
        entries: Rc<Vec<FileEntry>>,
        filtered: Vec<usize>,
        sel: usize,
        offset: i32,
        marked: Vec<bool>,
    },
}

/// Case-insensitive substring filter over entry names; empty filter
/// keeps everything. Returns indices into `entries` in source order.
pub(crate) fn filtered_indices(entries: &[FileEntry], filter: &str) -> Vec<usize> {
    if filter.is_empty() {
        return (0..entries.len()).collect();
    }
    let needle = filter.to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.name.to_lowercase().contains(&needle))
        .map(|(i, _)| i)
        .collect()
}

/// `dir` + entry name, with the platform-honest separator handling of
/// `std::path` (pure string work — no filesystem touch).
pub(crate) fn join_path(dir: &str, name: &str) -> String {
    std::path::Path::new(dir)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// Parent directory, `None` at a filesystem root (or a bare relative
/// component — the picker stays put there).
pub(crate) fn parent_path(dir: &str) -> Option<String> {
    let parent = std::path::Path::new(dir).parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }
    Some(parent.to_string_lossy().into_owned())
}

/// Width-aware LEFT truncation keeping the tail — the informative end
/// of a deep path (`…/c/d/e`). Sibling policy to
/// [`crate::text::truncate_ellipsis`] (which keeps the head).
pub(crate) fn left_truncate_ellipsis(s: &str, max_width: i32) -> String {
    if max_width <= 0 {
        return String::new();
    }
    if crate::text::width(s) <= max_width {
        return s.to_string();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    // Walk clusters from the END until the budget (minus the ellipsis
    // cell) is spent.
    let segs: Vec<_> = crate::text::segments(s).collect();
    let mut budget = max_width - 1;
    let mut start = segs.len();
    while start > 0 {
        let w = segs[start - 1].width;
        if w > budget {
            break;
        }
        budget -= w;
        start -= 1;
    }
    let byte = segs.get(start).map_or(s.len(), |seg| seg.offset);
    format!("…{}", &s[byte..])
}

/// Human size for the optional column: bytes below 1000, then one
/// decimal below 10 per unit (`1.4K`, `12K`, `3.0G`).
pub(crate) fn format_size(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    if n < 1000 {
        return format!("{n} B");
    }
    let mut v = n as f64;
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    if v < 10.0 {
        format!("{v:.1}{}", UNITS[unit])
    } else {
        format!("{v:.0}{}", UNITS[unit])
    }
}

/// Paint the list area from one snapshot. Dirs-first ordering, kind
/// glyphs and sorting are the SOURCE's business — this renders what it
/// was handed: `▸` accent for directories, `·` muted for files, the
/// selected row in the selection pair, `●` marks, the optional
/// right-aligned size column, and the shared scrollbar on overflow.
pub(crate) fn draw_rows(
    canvas: &mut dyn StyledCanvas,
    rect: Rect,
    content: &RowsContent,
    p: &PickerPalette,
    show_sizes: bool,
) {
    if rect.is_empty() {
        return;
    }
    let base = Style::new().fg(p.text).bg(p.ground);
    canvas.fill_styled(rect, ' ', &base);
    match content {
        RowsContent::Error(msg) => {
            let line = crate::text::truncate_ellipsis(&format!("cannot read: {msg}"), rect.w);
            canvas.print_styled(rect.origin(), &line, &Style::new().fg(p.error).bg(p.ground));
        }
        RowsContent::Empty => {
            let line = crate::text::truncate_ellipsis("empty directory", rect.w);
            canvas.print_styled(rect.origin(), &line, &Style::new().fg(p.faint).bg(p.ground));
        }
        RowsContent::NoMatches => {
            let line = crate::text::truncate_ellipsis("no matches", rect.w);
            canvas.print_styled(rect.origin(), &line, &Style::new().fg(p.faint).bg(p.ground));
        }
        RowsContent::Rows {
            entries,
            filtered,
            sel,
            offset,
            marked,
        } => {
            let total = filtered.len() as i32;
            let show_bar = total > rect.h;
            let text_w = if show_bar { rect.w - 1 } else { rect.w };
            let first = (*offset).clamp(0, (total - 1).max(0));
            for vis in 0..rect.h {
                let row = first + vis;
                if row >= total {
                    break;
                }
                let entry = &entries[filtered[row as usize]];
                let selected = row as usize == *sel;
                let y = rect.y + vis;
                let (name_fg, aux_fg, glyph_fg, mark_fg, bg) = if selected {
                    // Whole-row selection pair; auxiliary inks fold
                    // into sel_fg for readability on sel_bg.
                    (p.sel_fg, p.sel_fg, p.sel_fg, p.sel_fg, p.sel_bg)
                } else {
                    let name = if entry.is_dir { p.accent } else { p.text };
                    let glyph = if entry.is_dir { p.accent } else { p.muted };
                    (name, p.muted, glyph, p.mark, p.ground)
                };
                if selected {
                    canvas.fill_styled(
                        Rect::new(rect.x, y, text_w, 1),
                        ' ',
                        &Style::new().fg(p.sel_fg).bg(p.sel_bg),
                    );
                }
                let mut x = rect.x;
                if let Some(&is_marked) = marked.get(row as usize) {
                    // Mark column exists only in multi-select (the
                    // widget passes an empty vec otherwise).
                    let glyph = if is_marked { "●" } else { " " };
                    canvas.print_styled(Point::new(x, y), glyph, &Style::new().fg(mark_fg).bg(bg));
                    x += 2;
                }
                let kind = if entry.is_dir { "▸" } else { "·" };
                canvas.print_styled(Point::new(x, y), kind, &Style::new().fg(glyph_fg).bg(bg));
                x += 2;
                // Size column (files only, opt-in): right-aligned,
                // one gap cell from the name.
                let mut name_w = (rect.x + text_w - x).max(0);
                if show_sizes && !entry.is_dir {
                    if let Some(size) = entry.size {
                        let s = format_size(size);
                        let sw = crate::text::width(&s);
                        if name_w > sw + 1 {
                            canvas.print_styled(
                                Point::new(rect.x + text_w - sw, y),
                                &s,
                                &Style::new().fg(aux_fg).bg(bg),
                            );
                            name_w -= sw + 1;
                        }
                    }
                }
                let shown = crate::text::truncate_ellipsis(&entry.name, name_w);
                canvas.print_styled(Point::new(x, y), &shown, &Style::new().fg(name_fg).bg(bg));
            }
            if show_bar {
                crate::widgets::list::draw_scrollbar(
                    canvas, rect, first, total, p.track, p.thumb, p.ground,
                );
            }
        }
    }
}

/// Paint the breadcrumb row: left-truncated current dir, plus the
/// marked-count badge on the right in multi-select.
pub(crate) fn draw_breadcrumb(
    canvas: &mut dyn StyledCanvas,
    rect: Rect,
    dir: &str,
    marked: usize,
    p: &PickerPalette,
) {
    if rect.is_empty() {
        return;
    }
    canvas.fill_styled(rect, ' ', &Style::new().fg(p.muted).bg(p.ground));
    let badge = if marked > 0 {
        format!("{marked} marked")
    } else {
        String::new()
    };
    let badge_w = crate::text::width(&badge);
    let path_w = if badge_w > 0 {
        (rect.w - badge_w - 1).max(0)
    } else {
        rect.w
    };
    let path = left_truncate_ellipsis(dir, path_w);
    canvas.print_styled(rect.origin(), &path, &Style::new().fg(p.muted).bg(p.ground));
    if badge_w > 0 && rect.w > badge_w {
        canvas.print_styled(
            Point::new(rect.right() - badge_w, rect.y),
            &badge,
            &Style::new().fg(p.accent).bg(p.ground),
        );
    }
}
