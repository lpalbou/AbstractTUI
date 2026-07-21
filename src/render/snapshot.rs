//! Human-readable surface dumps for debugging draws.
//!
//! Two levels, both cheap and allocation-honest (they build one String;
//! never used on frame paths):
//!
//! - [`snapshot`]: the character grid inside a border — answers "WHERE
//!   did my text land" at a glance;
//! - [`snapshot_styles`]: the grid plus per-row style-run annotations —
//!   answers "WHY is this cell not bold/colored/linked" without a
//!   debugger.
//!
//! The grid renders wide glyphs once at their leader (they naturally
//! occupy two columns of the dump), pooled glyphs resolve through the
//! surface's pool, and `Cell::EMPTY` prints as a space (indistinguishable
//! from a drawn space by design — that is also true on screen; the style
//! dump is where the difference shows).

use std::fmt::Write as _;

use crate::base::Rgba;

use super::cell::{Attrs, Cell};
use super::surface::Surface;

/// The character grid inside a `+--+` border.
///
/// ```
/// use abstracttui::base::Size;
/// use abstracttui::render::{snapshot, Cell, Style, Surface};
///
/// let mut s = Surface::new(Size::new(8, 2), Cell::EMPTY);
/// s.draw_text(0, 0, "hi 世界", Style::new());
/// let dump = snapshot(&s);
/// assert_eq!(dump.lines().nth(1), Some("|hi 世界 |"));
/// ```
pub fn snapshot(s: &Surface) -> String {
    let mut out = String::new();
    let border: String = "-".repeat(s.width().max(0) as usize);
    let _ = writeln!(out, "+{border}+");
    for y in 0..s.height() {
        out.push('|');
        push_row_text(s, y, &mut out);
        out.push_str("|\n");
    }
    let _ = writeln!(out, "+{border}+");
    out
}

/// [`snapshot`] plus one annotation block per row: maximal same-style
/// runs as `cols fg bg attrs [ul=..] [link=..]`, skipping fully default
/// runs (unstyled ground stays quiet so the styled content stands out).
///
/// Colors print as `#rrggbb` (`#rrggbb/aa` when translucent) or `-` for
/// "terminal default" (alpha 0); attributes as their `Attrs` letters
/// (e.g. `B`, `BU`); links as the resolved URI when registered.
pub fn snapshot_styles(s: &Surface) -> String {
    let mut out = snapshot(s);
    for y in 0..s.height() {
        let mut row_header = false;
        let mut x = 0;
        while x < s.width() {
            let Some(&cell) = s.get(x, y) else { break };
            // Group the maximal run sharing this cell's paint.
            let start = x;
            while x < s.width() {
                match s.get(x, y) {
                    Some(c) if same_paint(c, &cell) => x += 1,
                    _ => break,
                }
            }
            if is_default_paint(&cell) {
                continue; // unstyled ground: not worth a line
            }
            if !row_header {
                let _ = writeln!(out, "row {y}:");
                row_header = true;
            }
            let _ = write!(
                out,
                "  {start}..{x}  fg={} bg={}",
                color(cell.fg),
                color(cell.bg)
            );
            if !cell.attrs.is_empty() {
                let _ = write!(out, " attrs={}", attrs(cell.attrs));
            }
            if !cell.ul.is_transparent() {
                let _ = write!(out, " ul={}", color(cell.ul));
            }
            if cell.link != 0 {
                match s.link_uri(cell.link) {
                    Some(uri) => {
                        let _ = write!(out, " link={uri}");
                    }
                    None => {
                        let _ = write!(out, " link=#{}", cell.link);
                    }
                }
            }
            out.push('\n');
        }
    }
    out
}

fn push_row_text(s: &Surface, y: i32, out: &mut String) {
    for x in 0..s.width() {
        let Some(c) = s.get(x, y) else { continue };
        if c.is_continuation() {
            continue; // the leader renders both columns
        }
        let g = s.glyph_str(c);
        out.push_str(if g.is_empty() { " " } else { g });
    }
}

fn same_paint(a: &Cell, b: &Cell) -> bool {
    a.fg == b.fg && a.bg == b.bg && a.ul == b.ul && a.attrs == b.attrs && a.link == b.link
}

fn is_default_paint(c: &Cell) -> bool {
    same_paint(c, &Cell::EMPTY)
}

fn color(c: Rgba) -> String {
    if c.is_transparent() {
        return "-".to_string();
    }
    if c.a == 255 {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    } else {
        format!("#{:02x}{:02x}{:02x}/{:02x}", c.r, c.g, c.b, c.a)
    }
}

/// One letter per attribute, emission order: **B**old, **d**im,
/// **I**talic, **U**nderline, **~**undercurl, **k**blink, **R**everse,
/// **h**idden, **S**trike.
fn attrs(a: Attrs) -> String {
    const LETTERS: [(Attrs, char); 9] = [
        (Attrs::BOLD, 'B'),
        (Attrs::DIM, 'd'),
        (Attrs::ITALIC, 'I'),
        (Attrs::UNDERLINE, 'U'),
        (Attrs::UNDERCURL, '~'),
        (Attrs::BLINK, 'k'),
        (Attrs::REVERSE, 'R'),
        (Attrs::HIDDEN, 'h'),
        (Attrs::STRIKE, 'S'),
    ];
    LETTERS
        .iter()
        .filter(|(bit, _)| a.contains(*bit))
        .map(|&(_, ch)| ch)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::render::style::Style;

    #[test]
    fn grid_shows_chars_wide_pairs_and_border() {
        let mut s = Surface::new(Size::new(8, 2), Cell::EMPTY);
        s.draw_text(0, 0, "hi 世界", Style::new());
        s.draw_text(0, 1, "x", Style::new());
        let dump = snapshot(&s);
        let lines: Vec<&str> = dump.lines().collect();
        assert_eq!(lines[0], "+--------+");
        assert_eq!(lines[1], "|hi 世界 |");
        assert_eq!(lines[2], "|x       |");
        assert_eq!(lines[3], "+--------+");
    }

    #[test]
    fn style_runs_annotate_only_styled_content() {
        let mut s = Surface::new(Size::new(12, 2), Cell::EMPTY);
        let link = s.register_link("https://x.io");
        s.draw_text(0, 0, "err", Style::new().fg(Rgba::rgb(255, 0, 0)).bold());
        s.draw_text(4, 0, "docs", Style::new().underline().link(link));
        let dump = snapshot_styles(&s);
        assert!(dump.contains("row 0:"), "{dump}");
        assert!(dump.contains("0..3  fg=#ff0000 bg=- attrs=B"), "{dump}");
        assert!(
            dump.contains("4..8  fg=- bg=- attrs=U link=https://x.io"),
            "{dump}"
        );
        assert!(!dump.contains("row 1:"), "unstyled row stays quiet: {dump}");
    }

    #[test]
    fn translucent_and_ul_colors_render_readably() {
        let mut s = Surface::new(Size::new(4, 1), Cell::EMPTY);
        s.draw_text(
            0,
            0,
            "ab",
            Style::new()
                .bg(Rgba::new(10, 20, 30, 128))
                .underline()
                .underline_color(Rgba::rgb(0, 0, 255)),
        );
        let dump = snapshot_styles(&s);
        assert!(dump.contains("bg=#0a141e/80"), "{dump}");
        assert!(dump.contains("ul=#0000ff"), "{dump}");
    }
}
