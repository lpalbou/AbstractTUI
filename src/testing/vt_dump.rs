//! Deterministic textual views of a [`super::vt::VtScreen`] — the
//! currency of golden snapshots and assert messages.
//!
//! OWNER: REDTEAM. Split from `vt.rs` to keep the interpreter readable;
//! everything here is a pure read through the screen's accessors.

use super::grid::Paint;
use super::vt::VtScreen;

impl VtScreen {
    /// Plain text: one line per row, trailing blanks trimmed.
    pub fn to_text(&self) -> String {
        let grid = self.grid_ref();
        let mut out = String::new();
        for y in 0..grid.h {
            out.push_str(grid.row_text(y).trim_end());
            out.push('\n');
        }
        out
    }

    /// Deterministic styled dump for golden snapshots: a header (size,
    /// cursor, pending-wrap marker, mode flags), bordered text rows, then
    /// per-row style runs (only non-default paints listed).
    pub fn to_styled_dump(&self) -> String {
        let grid = self.grid_ref();
        let cursor = self.cursor();
        let mut out = String::new();
        out.push_str(&format!(
            "size={}x{} cursor={},{}{} modes=[{}]\n",
            grid.w,
            grid.h,
            cursor.x,
            cursor.y,
            if self.wrap_is_pending() { "+wrap" } else { "" },
            self.modes()
                .all_set()
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(","),
        ));
        for y in 0..grid.h {
            out.push_str(&format!("{y:>3}|{}|\n", grid.row_text(y)));
        }
        let mut styles = String::new();
        for y in 0..grid.h {
            let mut x = 0;
            while x < grid.w {
                let paint = grid.cell(x, y).map(|c| c.paint).unwrap_or_default();
                let mut end = x + 1;
                while end < grid.w && grid.cell(end, y).map(|c| c.paint) == Some(paint) {
                    end += 1;
                }
                if !paint.is_default() {
                    styles.push_str(&format!("  {y}:{x}..{end} {}\n", paint_label(&paint)));
                }
                x = end;
            }
        }
        if !styles.is_empty() {
            out.push_str("styles:\n");
            out.push_str(&styles);
        }
        out
    }
}

fn paint_label(p: &Paint) -> String {
    let mut parts = Vec::new();
    if let Some(fg) = p.fg {
        parts.push(format!("fg={}", fg.to_hex()));
    }
    if let Some(bg) = p.bg {
        parts.push(format!("bg={}", bg.to_hex()));
    }
    if let Some(ul) = p.ul {
        parts.push(format!("ul={}", ul.to_hex()));
    }
    if !p.attrs.is_empty() {
        parts.push(format!("attrs={}", p.attrs.label()));
    }
    if let Some(l) = p.link {
        parts.push(format!("link={l}"));
    }
    parts.join(" ")
}
