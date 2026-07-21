//! Drawing contract between the ui layer and the render layer.
//!
//! CONTRACT(RENDER): `render::Surface` should implement `Canvas` when it
//! lands (request filed in reviews/cycle1/react-requests.md). The ui
//! layer draws through this trait so element draw closures, widgets and
//! tests are independent of the concrete surface: the render layer can
//! evolve cells (grapheme pools, attributes, hyperlinks) behind it.
//! Cycle-1 scope is deliberately minimal — enough for widgets to fill
//! regions and print styled text; rich attributes ride in cycle 2.

use crate::base::{Point, Rect, Rgba, Size};

/// Styled drawing: the extension trait richer widgets use (bold labels,
/// reverse-video selections, links). `render::Style` is the patch
/// vocabulary (fg/bg `None` = keep, attrs add/remove) — the same type
/// widgets pass to surfaces, so there is exactly one styling language.
///
/// Kept SEPARATE from `Canvas` (RENDER's request, adopted): the base
/// trait stays attr-free and stable; implementors opt into fidelity by
/// overriding these defaults (which degrade to fg/bg-only so a plain
/// canvas is one empty `impl StyledCanvas for X {}` away).
pub trait StyledCanvas: Canvas {
    /// Print with full styling. Default: fg/bg extracted, attrs dropped.
    fn print_styled(&mut self, p: Point, text: &str, style: &crate::render::Style) -> i32 {
        self.print(
            p,
            text,
            style.fg.unwrap_or(Rgba::WHITE),
            style.bg.unwrap_or(Rgba::TRANSPARENT),
        )
    }

    /// Fill a rect with a styled character. Default: per-row prints.
    fn fill_styled(&mut self, rect: Rect, ch: char, style: &crate::render::Style) {
        let mut buf = [0u8; 4];
        let s: &str = ch.encode_utf8(&mut buf);
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                self.print_styled(Point::new(x, y), s, style);
            }
        }
    }
}

/// Minimal cell-drawing surface. Coordinates are absolute;
/// implementations clip out-of-bounds writes (never panic).
pub trait Canvas {
    fn size(&self) -> Size;

    /// Put one character at `p`. `bg` with alpha 0 leaves the existing
    /// background (text over an inherited fill).
    fn put(&mut self, p: Point, ch: char, fg: Rgba, bg: Rgba);

    /// Fill a rect with a character (usually space) and colors.
    fn fill(&mut self, rect: Rect, ch: char, fg: Rgba, bg: Rgba) {
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                self.put(Point::new(x, y), ch, fg, bg);
            }
        }
    }

    /// Print a string starting at `p` (no wrapping; clipped by the
    /// implementation). Returns the number of cells advanced.
    ///
    /// v1 treats each `char` as one cell. Wide-glyph correctness belongs
    /// to the text layer (measure + render agree there); the ui layer
    /// must not hand-roll width tables.
    fn print(&mut self, p: Point, text: &str, fg: Rgba, bg: Rgba) -> i32 {
        let mut x = p.x;
        for ch in text.chars() {
            self.put(Point::new(x, p.y), ch, fg, bg);
            x += 1;
        }
        x - p.x
    }
}

/// A plain in-memory canvas for unit tests and headless runs. Kept here
/// (not in `testing/`) because the ui layer's own tests need it and the
/// testing rig belongs to another owner.
pub struct BufferCanvas {
    size: Size,
    cells: Vec<(char, Rgba, Rgba)>,
    attrs: Vec<crate::render::Attrs>,
}

impl BufferCanvas {
    pub fn new(size: Size) -> Self {
        let n = (size.w.max(0) * size.h.max(0)) as usize;
        BufferCanvas {
            size,
            cells: vec![(' ', Rgba::WHITE, Rgba::TRANSPARENT); n],
            attrs: vec![crate::render::Attrs::NONE; n],
        }
    }

    pub fn cell(&self, p: Point) -> Option<(char, Rgba, Rgba)> {
        if p.x < 0 || p.y < 0 || p.x >= self.size.w || p.y >= self.size.h {
            return None;
        }
        Some(self.cells[(p.y * self.size.w + p.x) as usize])
    }

    /// Attributes recorded by styled writes (kept in a parallel grid so
    /// the `cell()` tuple stays stable for existing tests). `Attrs::NONE`
    /// for plain writes and out-of-bounds.
    pub fn attrs_at(&self, p: Point) -> crate::render::Attrs {
        if p.x < 0 || p.y < 0 || p.x >= self.size.w || p.y >= self.size.h {
            return crate::render::Attrs::NONE;
        }
        self.attrs[(p.y * self.size.w + p.x) as usize]
    }

    /// Row text (chars only) — handy for golden-string assertions.
    pub fn row_text(&self, y: i32) -> String {
        (0..self.size.w)
            .map(|x| self.cell(Point::new(x, y)).map(|c| c.0).unwrap_or(' '))
            .collect()
    }
}

impl Canvas for BufferCanvas {
    fn size(&self) -> Size {
        self.size
    }

    fn put(&mut self, p: Point, ch: char, fg: Rgba, bg: Rgba) {
        if p.x < 0 || p.y < 0 || p.x >= self.size.w || p.y >= self.size.h {
            return; // clip, never panic
        }
        let idx = (p.y * self.size.w + p.x) as usize;
        let prev = self.cells[idx];
        // Alpha-0 bg means "keep what's under me" — matches compositor
        // semantics so tests reflect real layering behavior.
        let bg = if bg.is_transparent() { prev.2 } else { bg };
        self.cells[idx] = (ch, fg, bg);
        self.attrs[idx] = crate::render::Attrs::NONE; // fresh plain write
    }
}

impl StyledCanvas for BufferCanvas {
    fn print_styled(&mut self, p: Point, text: &str, style: &crate::render::Style) -> i32 {
        let advanced = self.print(
            p,
            text,
            style.fg.unwrap_or(Rgba::WHITE),
            style.bg.unwrap_or(Rgba::TRANSPARENT),
        );
        // Record the attr patch over the written span so tests can assert
        // BOLD/REVERSE/UNDERLINE placement.
        let attrs = crate::render::Attrs::NONE
            .without(style.remove)
            .with(style.add);
        for x in p.x..p.x + advanced {
            if x >= 0 && x < self.size.w && p.y >= 0 && p.y < self.size.h {
                self.attrs[(p.y * self.size.w + x) as usize] = attrs;
            }
        }
        advanced
    }
}

/// Adapter: draw through `ui::Canvas` into a `render::Surface` (the root
/// compositor layer). The surface tracks its own damage on every write,
/// so painting here feeds the flatten/diff pipeline with no extra
/// bookkeeping.
///
/// RENDER may later implement `Canvas` on `Surface` directly (request
/// filed); this adapter is the app-side default meanwhile and stays as
/// the place where ui color conventions (alpha-0 = inherit) translate
/// into `render::Style` patch semantics.
pub struct SurfaceCanvas<'a> {
    surface: &'a mut crate::render::Surface,
}

impl<'a> SurfaceCanvas<'a> {
    pub fn new(surface: &'a mut crate::render::Surface) -> Self {
        SurfaceCanvas { surface }
    }

    /// ui alpha-0 = "keep what's underneath" maps onto `Style`'s patch
    /// `None` = "keep the cell's current value".
    fn style_for(fg: Rgba, bg: Rgba) -> crate::render::Style {
        let mut style = crate::render::Style::new();
        if !fg.is_transparent() {
            style = style.fg(fg);
        }
        if !bg.is_transparent() {
            style = style.bg(bg);
        }
        style
    }
}

impl Canvas for SurfaceCanvas<'_> {
    fn size(&self) -> Size {
        self.surface.size()
    }

    fn put(&mut self, p: Point, ch: char, fg: Rgba, bg: Rgba) {
        // Encode without allocating; draw_text handles width (wide chars
        // become leader+continuation pairs) and clipping.
        let mut buf = [0u8; 4];
        let s: &str = ch.encode_utf8(&mut buf);
        self.surface.draw_text(p.x, p.y, s, Self::style_for(fg, bg));
    }

    fn fill(&mut self, rect: Rect, ch: char, fg: Rgba, bg: Rgba) {
        // A space fill is the common erase: use the surface's rect fill
        // (cheaper, and it repairs wide pairs at the edges itself).
        if ch == ' ' {
            let cell = crate::render::Cell::new(crate::render::Glyph::SPACE)
                .with_fg(fg)
                .with_bg(bg);
            self.surface.fill_rect(rect, cell);
            return;
        }
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                self.put(Point::new(x, y), ch, fg, bg);
            }
        }
    }

    fn print(&mut self, p: Point, text: &str, fg: Rgba, bg: Rgba) -> i32 {
        // The surface measures with text::cluster_width — the ONE width
        // authority — so ui-level printing agrees with rendering.
        self.surface
            .draw_text(p.x, p.y, text, Self::style_for(fg, bg))
    }
}

impl StyledCanvas for SurfaceCanvas<'_> {
    /// Full fidelity: the style patch goes straight to the surface —
    /// attrs, links, underline color all survive.
    fn print_styled(&mut self, p: Point, text: &str, style: &crate::render::Style) -> i32 {
        self.surface.draw_text(p.x, p.y, text, *style)
    }

    fn fill_styled(&mut self, rect: Rect, ch: char, style: &crate::render::Style) {
        if ch == ' ' {
            let cell = style.apply(&crate::render::Cell::new(crate::render::Glyph::SPACE));
            self.surface.fill_rect(rect, cell);
            return;
        }
        let mut buf = [0u8; 4];
        let s: &str = ch.encode_utf8(&mut buf);
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                self.surface.draw_text(x, y, s, *style);
            }
        }
    }
}

/// Clips every write to `clip`. Phase D wraps the frame canvas in one of
/// these per damage rect, so a widget painting outside the damaged region
/// costs nothing and a widget painting outside its OWN rect cannot smear
/// neighbors during a partial repaint.
pub struct ClippedCanvas<'a> {
    inner: &'a mut dyn StyledCanvas,
    clip: Rect,
}

impl<'a> ClippedCanvas<'a> {
    pub fn new(inner: &'a mut dyn StyledCanvas, clip: Rect) -> Self {
        ClippedCanvas { inner, clip }
    }
}

impl Canvas for ClippedCanvas<'_> {
    fn size(&self) -> Size {
        self.inner.size()
    }

    fn put(&mut self, p: Point, ch: char, fg: Rgba, bg: Rgba) {
        if self.clip.contains(p) {
            self.inner.put(p, ch, fg, bg);
        }
    }

    fn fill(&mut self, rect: Rect, ch: char, fg: Rgba, bg: Rgba) {
        let clipped = rect.intersect(self.clip);
        if !clipped.is_empty() {
            self.inner.fill(clipped, ch, fg, bg);
        }
    }

    fn print(&mut self, p: Point, text: &str, fg: Rgba, bg: Rgba) -> i32 {
        if p.y < self.clip.y || p.y >= self.clip.bottom() {
            return 0;
        }
        let total = crate::text::width(text);
        if p.x >= self.clip.x && p.x + total <= self.clip.right() {
            // Fully inside: one delegated print keeps wide-glyph handling
            // in the underlying canvas.
            return self.inner.print(p, text, fg, bg);
        }
        // Straddles the clip edge: walk chars (std only in this layer),
        // widths from the engine's ONE authority via per-char lookup.
        // A wide char half-in half-out is dropped (blank edge beats a
        // smeared half glyph). Multi-scalar clusters (ZWJ emoji) may
        // mis-slice on this degraded edge path — labeled limitation;
        // fully-inside prints (the overwhelmingly common case) delegate
        // whole and stay cluster-correct.
        let mut x = p.x;
        let mut buf = [0u8; 4];
        for ch in text.chars() {
            let s: &str = ch.encode_utf8(&mut buf);
            let w = crate::text::cluster_width(s);
            if w <= 0 {
                continue;
            }
            if x >= self.clip.x && x + w <= self.clip.right() {
                self.inner.print(Point::new(x, p.y), s, fg, bg);
            }
            x += w;
        }
        x - p.x
    }
}

impl StyledCanvas for ClippedCanvas<'_> {
    fn print_styled(&mut self, p: Point, text: &str, style: &crate::render::Style) -> i32 {
        if p.y < self.clip.y || p.y >= self.clip.bottom() {
            return 0;
        }
        let total = crate::text::width(text);
        if p.x >= self.clip.x && p.x + total <= self.clip.right() {
            return self.inner.print_styled(p, text, style);
        }
        // Edge path: per-char, same degradation notes as `print`.
        let mut x = p.x;
        let mut buf = [0u8; 4];
        for ch in text.chars() {
            let s: &str = ch.encode_utf8(&mut buf);
            let w = crate::text::cluster_width(s);
            if w <= 0 {
                continue;
            }
            if x >= self.clip.x && x + w <= self.clip.right() {
                self.inner.print_styled(Point::new(x, p.y), s, style);
            }
            x += w;
        }
        x - p.x
    }

    fn fill_styled(&mut self, rect: Rect, ch: char, style: &crate::render::Style) {
        let clipped = rect.intersect(self.clip);
        if !clipped.is_empty() {
            self.inner.fill_styled(clipped, ch, style);
        }
    }
}
