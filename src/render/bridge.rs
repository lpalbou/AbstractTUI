//! Bridges from sibling drawing vocabularies into [`Surface`].
//!
//! Two producers speak simpler cell languages than the render core:
//!
//! - the ui layer draws through `ui::Canvas` (char + fg + bg, absolute
//!   coords, clipped) — implemented for `Surface` here so widgets stay
//!   independent of the concrete cell model;
//! - the gfx layer emits mosaic cell patches (`gfx::mosaic::CellPatch`
//!   is `{ pos, ch, fg, bg }`). Render must not import gfx (siblings),
//!   so [`Surface::blit_mosaic`] accepts plain `(Point, char, Rgba,
//!   Rgba)` tuples — exactly what `gfx::mosaic::blit_into` produces, one
//!   `map` away. The tuple is the contract; either side can refactor
//!   internals without touching the other.
//!
//! Both bridges write ATOMIC cells (attrs cleared, no link, default
//! underline color): a bridge caller paints fresh content, and inheriting
//! a stale BOLD or hyperlink from whatever was underneath would be a
//! correctness surprise. Rich styling stays native `Surface` API
//! (`draw_text` + `Style`), which patches instead of replacing.

// Note on the dependency arrow: `render` sits below `ui` in the layer
// map, but the `Canvas` trait is ui-owned and the `Surface` type is
// render-owned — the impl must live on one side, and this side is the one
// RENDER can keep correct as the cell model evolves (REACT request 2).
// The import is trait-only; render never touches ui's tree/view/event
// machinery. If the trait migrates to `base` later, only this line moves.
use crate::base::{Point, Rect, Rgba};
use crate::ui::{Canvas, StyledCanvas};

use super::cell::{Cell, Glyph, GlyphPool};
use super::style::Style;
use super::surface::Surface;

/// Glyph for one `char`. Chars are ≤ 4 UTF-8 bytes, always inside the
/// 10-byte inline window, so the pool is provably never written — a
/// throwaway empty pool (no allocation) satisfies the signature without
/// touching the surface's real pool.
fn char_glyph(ch: char) -> Option<Glyph> {
    let mut buf = [0u8; 4];
    let mut scratch = GlyphPool::default();
    let g = Glyph::new(ch.encode_utf8(&mut buf), &mut scratch);
    debug_assert!(scratch.is_empty(), "a char can never spill to the pool");
    g
}

impl Canvas for Surface {
    fn size(&self) -> crate::base::Size {
        Surface::size(self)
    }

    /// One char at `p`. Per the trait contract, `bg` with alpha 0 keeps
    /// the existing background; everything else about the cell is
    /// replaced (fresh draw semantics — matches `BufferCanvas`). Wide
    /// chars occupy their pair; control chars are stripped.
    fn put(&mut self, p: Point, ch: char, fg: Rgba, bg: Rgba) {
        let mut buf = [0u8; 4];
        let s: &str = ch.encode_utf8(&mut buf);
        let mut style = Style::absolute().fg(fg).underline_color(Rgba::TRANSPARENT);
        if !bg.is_transparent() {
            style = style.bg(bg);
        }
        // draw_text carries the whole put contract: width policy, pair
        // invariants, clipping, control stripping.
        self.draw_text(p.x, p.y, s, style);
    }

    fn fill(&mut self, rect: Rect, ch: char, fg: Rgba, bg: Rgba) {
        // Control/zero-width fill chars degrade to a plain space rather
        // than silently doing nothing (a fill is a paint, not a print).
        let glyph = char_glyph(ch).unwrap_or(Glyph::SPACE);
        let cell = Cell::new(glyph).with_fg(fg).with_bg(bg);
        self.fill_rect(rect, cell);
    }

    /// Grapheme-correct print (overrides the trait's one-char-one-cell
    /// default): wide glyphs advance two columns, ZWJ sequences stay
    /// whole. Returns real columns advanced.
    fn print(&mut self, p: Point, text: &str, fg: Rgba, bg: Rgba) -> i32 {
        let mut style = Style::absolute().fg(fg).underline_color(Rgba::TRANSPARENT);
        if !bg.is_transparent() {
            style = style.bg(bg);
        }
        self.draw_text(p.x, p.y, text, style)
    }
}

/// Styled-canvas resolution [C8, FROZEN]: `ui::StyledCanvas` is THE
/// styled drawing trait. The render-side duplicate this file once
/// declared (same name, drifted signature, zero consumers) is DELETED —
/// two traits sharing one name across layers is a docs landmine.
/// `Surface` implements the ui trait directly with full fidelity, so
/// `&mut Surface` slots into any `&mut dyn StyledCanvas` parameter
/// (widget draw closures, overlay painters) without the `SurfaceCanvas`
/// wrapper. The wrapper remains ui's adapter for callers holding only a
/// `&mut Surface` borrow behind other ui plumbing; both routes end in
/// [`Surface::draw_text`], the one canonical rich-draw call.
impl StyledCanvas for Surface {
    /// Full fidelity: the style patch goes straight to the surface —
    /// attrs, links, underline color all survive. Grapheme-correct
    /// (wide pairs, ZWJ clusters); returns real columns advanced.
    fn print_styled(&mut self, p: Point, text: &str, style: &Style) -> i32 {
        self.draw_text(p.x, p.y, text, *style)
    }

    /// Fills `rect` with `ch` carrying `style`'s paint. The style is a
    /// PATCH over a fresh cell (fg `None` = terminal default, not "keep
    /// what was there" — a fill replaces). Space fills take the cheap
    /// `fill_rect` path; other chars honor the width policy per cell.
    fn fill_styled(&mut self, rect: Rect, ch: char, style: &Style) {
        if ch == ' ' {
            self.fill_rect(rect, style.apply(&Cell::new(Glyph::SPACE)));
            return;
        }
        let mut buf = [0u8; 4];
        let s: &str = ch.encode_utf8(&mut buf);
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                self.draw_text(x, y, s, *style);
            }
        }
    }
}

impl Surface {
    /// Writes gfx mosaic patches at `origin`. Input is the neutral tuple
    /// shape `(pos, ch, fg, bg)` — `gfx::mosaic::CellPatch` fields in
    /// order; feed it with
    /// `patches.iter().map(|p| (p.pos, p.ch, p.fg, p.bg))`.
    ///
    /// Semantics (per the gfx grid contract):
    /// - every patch is applied, including fully transparent ones — a
    ///   transparent patch means "image empty here" and writes the
    ///   see-through [`Cell::EMPTY`], clearing stale content while letting
    ///   lower layers show;
    /// - visible patches replace the cell wholesale (mosaic is image
    ///   data; inheriting text attrs/links underneath it would be wrong);
    /// - writes are clipped; damage is one bounding rect, not per-cell.
    pub fn blit_mosaic<I>(&mut self, patches: I, origin: Point)
    where
        I: IntoIterator<Item = (Point, char, Rgba, Rgba)>,
    {
        let mut bounds: Option<Rect> = None;
        for (pos, ch, fg, bg) in patches {
            let x = origin.x + pos.x;
            let y = origin.y + pos.y;
            let cell = if fg.is_transparent() && bg.is_transparent() {
                Cell::EMPTY
            } else {
                // Mosaic glyphs (blocks, sextants, braille) are narrow
                // single chars; the width policy still runs so a hostile
                // char degrades safely (control -> styled blank).
                match char_glyph(ch) {
                    Some(glyph) => Cell::new(glyph).with_fg(fg).with_bg(bg),
                    None => Cell::EMPTY.with_fg(fg).with_bg(bg),
                }
            };
            let (x0, x1) = self.set_quiet(x, y, cell);
            if x1 > x0 {
                let r = Rect::new(x0, y, x1 - x0, 1);
                bounds = Some(match bounds {
                    Some(b) => b.union(r),
                    None => r,
                });
            }
        }
        if let Some(b) = bounds {
            // ±1 column: set_quiet's pair repairs land just outside the
            // written span.
            self.add_damage(Rect::new(b.x - 1, b.y, b.w + 2, b.h));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::render::cell::Attrs;

    fn surf(w: i32, h: i32) -> Surface {
        Surface::new(Size::new(w, h), Cell::EMPTY)
    }

    #[test]
    fn canvas_put_and_print_are_grapheme_correct() {
        let mut s = surf(10, 2);
        s.put(Point::new(0, 0), '世', Rgba::WHITE, Rgba::TRANSPARENT);
        assert!(s.get(1, 0).unwrap().is_continuation(), "wide char pairs");
        let advanced = Canvas::print(&mut s, Point::new(0, 1), "a世b", Rgba::WHITE, Rgba::BLACK);
        assert_eq!(advanced, 4, "print reports real columns");
        assert_eq!(s.glyph_str(s.get(1, 1).unwrap()), "世");
        s.debug_validate().unwrap();
    }

    #[test]
    fn canvas_bg_alpha0_keeps_underlying_and_clears_attrs() {
        let mut s = surf(4, 1);
        s.fill_rect(
            s.bounds(),
            Cell::EMPTY
                .with_bg(Rgba::rgb(1, 2, 3))
                .with_attrs(Attrs::BOLD),
        );
        s.put(Point::new(0, 0), 'x', Rgba::WHITE, Rgba::TRANSPARENT);
        let c = s.get(0, 0).unwrap();
        assert_eq!(c.bg, Rgba::rgb(1, 2, 3), "transparent bg keeps underlying");
        assert_eq!(c.attrs, Attrs::NONE, "fresh draw clears attrs");
        s.put(Point::new(1, 0), 'y', Rgba::WHITE, Rgba::rgb(9, 9, 9));
        assert_eq!(s.get(1, 0).unwrap().bg, Rgba::rgb(9, 9, 9));
    }

    #[test]
    fn canvas_clips_out_of_bounds() {
        let mut s = surf(3, 1);
        s.put(Point::new(-1, 0), 'a', Rgba::WHITE, Rgba::TRANSPARENT);
        s.put(Point::new(5, 0), 'b', Rgba::WHITE, Rgba::TRANSPARENT);
        s.put(Point::new(0, 7), 'c', Rgba::WHITE, Rgba::TRANSPARENT);
        assert_eq!(s.glyph_str(s.get(0, 0).unwrap()), "");
        s.debug_validate().unwrap();
    }

    #[test]
    fn mosaic_patches_apply_and_transparent_clears() {
        let mut s = surf(6, 2);
        s.draw_text(0, 0, "stale!", Style::new());
        let patches = [
            (
                Point::new(0, 0),
                '▀',
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(0, 0, 255),
            ),
            (
                Point::new(1, 0),
                '▀',
                Rgba::rgb(10, 10, 10),
                Rgba::TRANSPARENT,
            ),
            (Point::new(2, 0), ' ', Rgba::TRANSPARENT, Rgba::TRANSPARENT),
        ];
        s.blit_mosaic(patches.iter().copied(), Point::new(1, 0));
        assert_eq!(s.glyph_str(s.get(1, 0).unwrap()), "▀");
        assert_eq!(s.get(1, 0).unwrap().bg, Rgba::rgb(0, 0, 255));
        assert_eq!(
            s.get(2, 0).unwrap().bg,
            Rgba::TRANSPARENT,
            "image alpha rides through"
        );
        let cleared = s.get(3, 0).unwrap();
        assert!(
            cleared.glyph.is_empty(),
            "transparent patch clears stale glyph"
        );
        assert_eq!(
            s.glyph_str(s.get(0, 0).unwrap()),
            "s",
            "outside patch untouched"
        );
        s.debug_validate().unwrap();
    }

    #[test]
    fn mosaic_damages_one_bounding_rect() {
        let mut s = surf(40, 20);
        let mut sink = Vec::new();
        s.take_damage(&mut sink);
        sink.clear();
        let patches: Vec<_> = (0..64)
            .map(|i| {
                (
                    Point::new(i % 8, i / 8),
                    '▀',
                    Rgba::rgb(i as u8, 0, 0),
                    Rgba::rgb(0, i as u8, 0),
                )
            })
            .collect();
        s.blit_mosaic(patches.iter().copied(), Point::new(4, 3));
        s.take_damage(&mut sink);
        assert_eq!(sink.len(), 1, "one rect for the whole grid: {sink:?}");
        assert!(sink[0].contains(Point::new(4, 3)) && sink[0].contains(Point::new(11, 10)));
    }

    #[test]
    fn styled_canvas_prints_attrs_links_and_fills() {
        use crate::base::Rgba;
        let mut s = surf(12, 2);
        let link = s.register_link("https://example.com");
        let style = Style::new()
            .fg(Rgba::rgb(200, 0, 0))
            .attrs(Attrs::BOLD | Attrs::UNDERLINE)
            .underline_color(Rgba::rgb(0, 0, 200))
            .link(link);
        let advanced = StyledCanvas::print_styled(&mut s, Point::new(0, 0), "a世b", &style);
        assert_eq!(advanced, 4, "grapheme-correct advance");
        let c = s.get(0, 0).unwrap();
        assert!(c.attrs.contains(Attrs::BOLD));
        assert_eq!(c.ul, Rgba::rgb(0, 0, 200));
        assert_eq!(s.link_uri(c.link), Some("https://example.com"));
        s.debug_validate().unwrap();

        StyledCanvas::fill_styled(
            &mut s,
            Rect::new(0, 1, 4, 1),
            ' ',
            &Style::new().bg(Rgba::rgb(1, 2, 3)).attrs(Attrs::DIM),
        );
        let f = s.get(2, 1).unwrap();
        assert_eq!(f.bg, Rgba::rgb(1, 2, 3));
        assert!(f.attrs.contains(Attrs::DIM));
        assert_eq!(s.glyph_str(f), " ", "fill paints ground with a space");

        // Non-space fill honors the width policy per cell.
        StyledCanvas::fill_styled(
            &mut s,
            Rect::new(4, 1, 4, 1),
            '▒',
            &Style::new().fg(Rgba::rgb(9, 9, 9)),
        );
        assert_eq!(s.glyph_str(s.get(5, 1).unwrap()), "▒");
        s.debug_validate().unwrap();
    }

    #[test]
    fn surface_slots_into_dyn_styled_canvas() {
        // The C8 resolution's point: a bare Surface is a StyledCanvas —
        // widget-style draw closures take it without a wrapper.
        fn paint(c: &mut dyn StyledCanvas) {
            c.print_styled(Point::new(0, 0), "ok", &Style::new().attrs(Attrs::BOLD));
        }
        let mut s = surf(4, 1);
        paint(&mut s);
        assert!(s.get(0, 0).unwrap().attrs.contains(Attrs::BOLD));
    }

    #[test]
    fn mosaic_clips_and_survives_wide_content_underneath() {
        let mut s = surf(6, 1);
        s.draw_text(0, 0, "世界人", Style::new());
        // One patch over the continuation of 世.
        s.blit_mosaic(
            [(Point::new(0, 0), '▄', Rgba::WHITE, Rgba::BLACK)]
                .iter()
                .copied(),
            Point::new(1, 0),
        );
        s.debug_validate().unwrap();
        assert_eq!(
            s.glyph_str(s.get(0, 0).unwrap()),
            " ",
            "orphan leader blanked"
        );
        assert_eq!(s.glyph_str(s.get(1, 0).unwrap()), "▄");
        // Fully off-surface patches are clipped without panic.
        s.blit_mosaic(
            [(Point::new(50, 50), '▄', Rgba::WHITE, Rgba::BLACK)]
                .iter()
                .copied(),
            Point::ZERO,
        );
    }
}
