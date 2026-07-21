//! Block: the bordered panel primitive — a box with optional title,
//! optional surface fill, and a focus ring.
//!
//! ```ignore
//! use abstracttui::widgets::{Block, BorderKind, TitleAlign};
//! let t = theme.tokens;
//! let panel = Block::new()
//!     .border(BorderKind::Rounded)
//!     .title("Sessions")
//!     .title_align(TitleAlign::Left)
//!     .fill(t.surface)
//!     .focused(is_focused)
//!     .child(body_view)
//!     .element(&t)
//!     .build();
//! ```
//!
//! Tokens: border strokes use `border` (or `border_focus` when
//! `.focused(true)` — the focus ring rule), the title uses `text_muted`
//! (focused: `text`), fill is caller-chosen (pass `t.surface` for a panel,
//! omit to keep the underlying ground). Colors resolve at view build —
//! the draw closure captures plain `Rgba` (damage contract §5).
//!
//! OWNER: DESIGN.

use crate::base::{Point, Rect, Rgba};
use crate::layout::{Edges, Style as LayoutStyle};
use crate::theme::TokenSet;
use crate::ui::{Canvas, Element, View};

/// Border glyph families. `None` keeps layout parity (no padding) with
/// zero strokes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BorderKind {
    Plain,
    Rounded,
    Double,
    Heavy,
    None,
}

impl BorderKind {
    /// `[top-left, top, top-right, left, right, bottom-left, bottom,
    /// bottom-right]`
    fn glyphs(self) -> Option<[char; 8]> {
        match self {
            BorderKind::Plain => Some(['┌', '─', '┐', '│', '│', '└', '─', '┘']),
            BorderKind::Rounded => Some(['╭', '─', '╮', '│', '│', '╰', '─', '╯']),
            BorderKind::Double => Some(['╔', '═', '╗', '║', '║', '╚', '═', '╝']),
            BorderKind::Heavy => Some(['┏', '━', '┓', '┃', '┃', '┗', '━', '┛']),
            BorderKind::None => None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TitleAlign {
    Left,
    Center,
    Right,
}

pub struct Block {
    border: BorderKind,
    title: Option<String>,
    title_align: TitleAlign,
    focused: bool,
    fill: Option<Rgba>,
    shadow: Option<Rgba>,
    layout: LayoutStyle,
    children: Vec<View>,
}

impl Block {
    pub fn new() -> Block {
        Block {
            border: BorderKind::Plain,
            title: None,
            title_align: TitleAlign::Left,
            focused: false,
            fill: None,
            shadow: None,
            layout: LayoutStyle::default(),
            children: Vec::new(),
        }
    }

    pub fn border(mut self, kind: BorderKind) -> Block {
        self.border = kind;
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Block {
        self.title = Some(title.into());
        self
    }

    pub fn title_align(mut self, align: TitleAlign) -> Block {
        self.title_align = align;
        self
    }

    /// Focused blocks draw their border in `border_focus` — the engine's
    /// focus ring convention.
    pub fn focused(mut self, focused: bool) -> Block {
        self.focused = focused;
        self
    }

    /// Paint the interior with this ground (pass a surface token). Omit to
    /// keep whatever is beneath.
    pub fn fill(mut self, ground: Rgba) -> Block {
        self.fill = Some(ground);
        self
    }

    /// Elevation: a one-cell drop-shadow strip along the right + bottom
    /// edges (the panel visually lifts). Pass the theme's `shadow_ground`
    /// token — pre-composited at theme build, so the widget never does
    /// color math (RT1-9b). The panel's chrome shrinks by one cell each
    /// way to make room; cost is a one-time paint, not a per-frame effect.
    pub fn shadow(mut self, ground: Rgba) -> Block {
        self.shadow = Some(ground);
        self
    }

    /// Layout style for the element (size, grow, margin…). Padding is
    /// overridden to 1 when a border is drawn so children never overlap
    /// the strokes.
    pub fn layout(mut self, style: LayoutStyle) -> Block {
        self.layout = style;
        self
    }

    pub fn child(mut self, view: impl Into<View>) -> Block {
        self.children.push(view.into());
        self
    }

    /// Resolve tokens and build the element. Returned as [`Element`] so
    /// callers can still attach handlers/shortcuts before `.build()`.
    /// Canonical one-call build (RT8-3 uniformity): same shape as the
    /// interactive widgets — tokens resolve from the app's theme
    /// context, the finished `View` comes back. `element(&tokens)`
    /// remains the explicit-theming door.
    pub fn view(self, cx: crate::reactive::Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(&t).build()
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let stroke = if self.focused {
            t.border_focus
        } else {
            t.border
        };
        let title_fg = if self.focused { t.text } else { t.text_muted };
        let border = self.border;
        let title = self.title;
        let align = self.title_align;
        let fill = self.fill;
        let shadow = self.shadow;

        // Chrome insets ride a PROTECTED padding floor, not the plain
        // style: a caller's later `.style(grow)` on the returned Element
        // then sizes the panel WITHOUT dropping content onto the border
        // (RT8-7 — the worst first-use trap of the cycle-8 review).
        let mut chrome = Edges::ZERO;
        if border.glyphs().is_some() {
            chrome = Edges::all(1);
        }
        if shadow.is_some() {
            // The strip takes the last column/row; children stay inside
            // the lifted panel.
            chrome.right += 1;
            chrome.bottom += 1;
        }
        let layout = self.layout;

        let mut el =
            Element::new()
                .style(layout)
                .padding_floor(chrome)
                .draw(move |canvas, rect| {
                    let panel = if let Some(ground) = shadow {
                        let panel =
                            Rect::new(rect.x, rect.y, (rect.w - 1).max(0), (rect.h - 1).max(0));
                        // Offset strip: right column + bottom row, shifted one
                        // cell down-right — reads as light from the top-left.
                        for y in (rect.y + 1)..rect.bottom() {
                            canvas.put(Point::new(rect.right() - 1, y), ' ', ground, ground);
                        }
                        for x in (rect.x + 1)..rect.right() {
                            canvas.put(Point::new(x, rect.bottom() - 1), ' ', ground, ground);
                        }
                        panel
                    } else {
                        rect
                    };
                    draw_block(
                        canvas,
                        panel,
                        border,
                        stroke,
                        fill,
                        title.as_deref(),
                        title_fg,
                        align,
                    );
                });
        for child in self.children {
            el = el.child(child);
        }
        el
    }
}

impl Default for Block {
    fn default() -> Self {
        Block::new()
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_block(
    canvas: &mut dyn Canvas,
    rect: Rect,
    border: BorderKind,
    stroke: Rgba,
    fill: Option<Rgba>,
    title: Option<&str>,
    title_fg: Rgba,
    align: TitleAlign,
) {
    if rect.w <= 0 || rect.h <= 0 {
        return;
    }
    if let Some(ground) = fill {
        canvas.fill(rect, ' ', ground, ground);
    }
    let Some([tl, top, tr, left, right, bl, bottom, br]) = border.glyphs() else {
        return;
    };
    let keep = Rgba::TRANSPARENT; // alpha-0 bg: keep what's beneath
    let bg = fill.unwrap_or(keep);
    let (x0, y0) = (rect.x, rect.y);
    let (x1, y1) = (rect.right() - 1, rect.bottom() - 1);

    for x in (x0 + 1)..x1 {
        canvas.put(Point::new(x, y0), top, stroke, bg);
        canvas.put(Point::new(x, y1), bottom, stroke, bg);
    }
    for y in (y0 + 1)..y1 {
        canvas.put(Point::new(x0, y), left, stroke, bg);
        canvas.put(Point::new(x1, y), right, stroke, bg);
    }
    canvas.put(Point::new(x0, y0), tl, stroke, bg);
    canvas.put(Point::new(x1, y0), tr, stroke, bg);
    if rect.h > 1 {
        canvas.put(Point::new(x0, y1), bl, stroke, bg);
        canvas.put(Point::new(x1, y1), br, stroke, bg);
    }

    // Title rides the top stroke, padded, truncated to the available run.
    let Some(title) = title else { return };
    let avail = (rect.w - 4).max(0) as usize; // corners + one pad each side
    if avail == 0 || title.is_empty() {
        return;
    }
    let shown: String = title.chars().take(avail).collect();
    let w = shown.chars().count() as i32 + 2; // " title "
    let tx = match align {
        TitleAlign::Left => x0 + 1,
        TitleAlign::Center => x0 + (rect.w - w).max(0) / 2,
        TitleAlign::Right => x1 - w,
    }
    .clamp(x0 + 1, (x1 - w).max(x0 + 1));
    canvas.print(Point::new(tx, y0), &format!(" {shown} "), title_fg, bg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::theme::default_theme;
    use crate::widgets::test_util::{draw_into, row};

    const SIZE: Size = Size { w: 20, h: 5 };

    #[test]
    fn rounded_border_with_left_title() {
        let t = default_theme().tokens;
        let view = Block::new()
            .border(BorderKind::Rounded)
            .title("Log")
            .element(&t);
        let c = draw_into(view, SIZE);
        assert_eq!(row(&c, 0), format!("╭ Log {}╮", "─".repeat(13)));
        assert_eq!(row(&c, 4), format!("╰{}╯", "─".repeat(18)));
        assert_eq!(c.cell(crate::base::Point::new(0, 2)).unwrap().0, '│');
        // Border color is the border token; title is muted.
        assert_eq!(
            c.cell(crate::base::Point::new(5, 0)).unwrap().1,
            t.text_muted
        );
        assert_eq!(c.cell(crate::base::Point::new(0, 0)).unwrap().1, t.border);
    }

    #[test]
    fn focus_ring_switches_to_border_focus() {
        let t = default_theme().tokens;
        let view = Block::new().focused(true).element(&t);
        let c = draw_into(view, SIZE);
        assert_eq!(
            c.cell(crate::base::Point::new(0, 0)).unwrap().1,
            t.border_focus
        );
    }

    #[test]
    fn fill_paints_interior_and_none_border_draws_nothing() {
        let t = default_theme().tokens;
        let view = Block::new()
            .border(BorderKind::None)
            .fill(t.surface)
            .element(&t);
        let c = draw_into(view, SIZE);
        assert_eq!(row(&c, 0).trim(), "");
        assert_eq!(c.cell(crate::base::Point::new(3, 2)).unwrap().2, t.surface);
    }

    #[test]
    fn title_truncates_and_double_heavy_render() {
        let t = default_theme().tokens;
        let view = Block::new()
            .border(BorderKind::Double)
            .title("A very long title that cannot possibly fit")
            .element(&t);
        let c = draw_into(view, Size::new(12, 3));
        let top = row(&c, 0);
        assert!(top.starts_with('╔') && top.ends_with('╗'), "{top:?}");
        assert!(top.contains(" A very l "), "truncated to the run: {top:?}");

        let view = Block::new().border(BorderKind::Heavy).element(&t);
        let c = draw_into(view, Size::new(4, 2));
        assert_eq!(row(&c, 0), "┏━━┓");
        assert_eq!(row(&c, 1), "┗━━┛");
    }

    #[test]
    fn shadow_strip_lifts_the_panel() {
        let t = default_theme().tokens;
        let view = Block::new()
            .fill(t.surface)
            .shadow(t.shadow_ground)
            .element(&t);
        let c = draw_into(view, Size::new(10, 4));
        // Bottom-right strip wears the pre-composited shadow ground…
        assert_eq!(
            c.cell(crate::base::Point::new(9, 2)).unwrap().2,
            t.shadow_ground
        );
        assert_eq!(
            c.cell(crate::base::Point::new(5, 3)).unwrap().2,
            t.shadow_ground
        );
        // …the offset corner cell (0, bottom) stays untouched…
        assert_ne!(
            c.cell(crate::base::Point::new(0, 3)).unwrap().2,
            t.shadow_ground
        );
        // …and the panel chrome shrank to make room (border at w-2).
        assert_eq!(c.cell(crate::base::Point::new(8, 0)).unwrap().0, '┐');
        // No shadow: chrome spans the full rect.
        let view = Block::new().element(&t);
        let c = draw_into(view, Size::new(10, 4));
        assert_eq!(c.cell(crate::base::Point::new(9, 0)).unwrap().0, '┐');
    }

    #[test]
    fn degenerate_rects_never_panic() {
        let t = default_theme().tokens;
        for size in [
            Size::new(0, 0),
            Size::new(1, 1),
            Size::new(2, 1),
            Size::new(1, 4),
        ] {
            let view = Block::new().title("x").element(&t);
            let _ = draw_into(view, size);
        }
    }

    #[test]
    fn bordered_block_pads_children() {
        let t = default_theme().tokens;
        let el = Block::new().element(&t);
        // The stroke room rides the PROTECTED floor now (RT8-7), not
        // the plain style — mount applies it.
        assert_eq!(el.padding_floor, Some(Edges::all(1)));
        let el = Block::new().border(BorderKind::None).element(&t);
        assert_eq!(el.padding_floor, Some(Edges::ZERO));
    }

    #[test]
    fn rt8_7_user_style_on_block_element_keeps_the_border_inset() {
        // THE cycle-8 first-use trap: `.style(grow)` on the returned
        // Element used to clobber the border padding, dropping content
        // onto the frame. The floor survives it now.
        use crate::base::Size;
        use crate::reactive::create_root;
        use crate::ui::{text, BufferCanvas, UiTree};
        let t = default_theme().tokens;
        let mut tree = UiTree::new(Size::new(12, 4));
        let (root, ()) = create_root(|cx| {
            let view = Block::new()
                .title("Panel")
                .child(text("inner"))
                .element(&t)
                // The newcomer's line, verbatim:
                .style(crate::layout::LayoutStyle::default().grow(1.0))
                .build();
            tree.mount(cx, view);
        });
        let mut canvas = BufferCanvas::new(Size::new(12, 4));
        tree.draw(&mut canvas);
        let top = canvas.row_text(0);
        assert!(
            top.contains("Panel"),
            "title intact on the frame row: {top:?}"
        );
        assert!(
            canvas.row_text(1).contains("inner"),
            "content INSIDE the border, not on it: {:?}",
            canvas.row_text(1)
        );
        assert!(!top.contains("inner"), "content never lands on the frame");
        root.dispose();
    }
}
