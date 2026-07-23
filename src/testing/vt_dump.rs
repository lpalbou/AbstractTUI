//! Deterministic textual views of a [`super::vt::VtScreen`] — the
//! currency of golden snapshots and assert messages — plus the bridge
//! into the engine-wide [`Screenshot`] capture value.
//!
//! OWNER: REDTEAM. Split from `vt.rs` to keep the interpreter readable;
//! everything here is a pure read through the screen's accessors. The
//! screenshot bridge deliberately does not touch the INTERPRETATION
//! path — the VT model stays an independent referee; only the exported
//! value type is shared.

use crate::render::screenshot::Screenshot;
use crate::render::Attrs as RenderAttrs;

use super::grid::{Attrs, CellContent, Paint};
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

    /// The modeled screen as a [`Screenshot`] — the same capture value
    /// the live driver produces, so headless tests export text / ANSI /
    /// SVG artifacts from the bytes an app actually emitted
    /// (`CaptureTerm::screen().screenshot()`).
    ///
    /// Mapping notes: blanks and printed spaces both capture as the
    /// canonical space (visually and wire-identical); hyperlink ids are
    /// dropped (not part of a visual capture); protocol-image payloads
    /// are consumed by the model as counted, unmodeled frames, so a
    /// VT-side capture carries no [`Screenshot::pixel_regions`] — the
    /// driver-side capture is where placement bookkeeping lives.
    pub fn screenshot(&self) -> Screenshot {
        let grid = self.grid_ref();
        let mut cells = Vec::with_capacity((grid.w * grid.h).max(0) as usize);
        for y in 0..grid.h {
            for x in 0..grid.w {
                let Some(cell) = grid.cell(x, y) else {
                    continue;
                };
                let (text, width): (&str, u8) = match &cell.content {
                    CellContent::Blank => (" ", 1),
                    CellContent::Continuation => ("", 0),
                    CellContent::Text(s) => (s, if cell.is_wide_leader() { 2 } else { 1 }),
                };
                let p = cell.paint;
                cells.push(Screenshot::make_cell(
                    text,
                    width,
                    p.fg.map(opaque),
                    p.bg.map(opaque),
                    p.ul.map(opaque),
                    map_attrs(p.attrs),
                ));
            }
        }
        Screenshot::from_cells(self.size(), cells)
    }
}

/// The model's colors are already opaque (SGR carries RGB); this pins
/// the invariant so captures from both sources compare bit-equal.
fn opaque(c: crate::base::Rgba) -> crate::base::Rgba {
    crate::base::Rgba::rgb(c.r, c.g, c.b)
}

/// Explicit flag-by-flag mapping — the two `Attrs` types share names,
/// NOT bit positions (render orders by SGR code, the model grew
/// historically). Never bit-pun between them.
fn map_attrs(a: Attrs) -> RenderAttrs {
    const MAP: [(u16, RenderAttrs); 9] = [
        (Attrs::BOLD, RenderAttrs::BOLD),
        (Attrs::DIM, RenderAttrs::DIM),
        (Attrs::ITALIC, RenderAttrs::ITALIC),
        (Attrs::UNDERLINE, RenderAttrs::UNDERLINE),
        (Attrs::UNDERCURL, RenderAttrs::UNDERCURL),
        (Attrs::BLINK, RenderAttrs::BLINK),
        (Attrs::REVERSE, RenderAttrs::REVERSE),
        (Attrs::HIDDEN, RenderAttrs::HIDDEN),
        (Attrs::STRIKE, RenderAttrs::STRIKE),
    ];
    let mut out = RenderAttrs::NONE;
    for (bit, mapped) in MAP {
        if a.contains(bit) {
            out = out.with(mapped);
        }
    }
    out
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
