//! Block: the bordered panel primitive — a box with optional title,
//! optional surface fill, a focus ring, and an opt-in close affordance.
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
//!     .on_close(move || open.set(false)) // ✕ on the title row (opt-in)
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
//! ## The close affordance (`on_close`)
//!
//! Opt-in per panel — whether a panel can be closed is the APP's
//! decision. A muted `✕` rides the title row's right end (inside the
//! border corner); hover tints it `error` (a consequence-bearing
//! action, the diff-vocabulary precedent), press adds BOLD. Activation
//! is MOUSE-ONLY and follows the Button convention: press + release
//! with the release still inside the run — never fire-on-down. The
//! affordance is deliberately NOT focusable (a focusable ✕ steals a
//! panel's first-focus from its content — the drawer 0.2.12 lesson);
//! keyboard close stays app-side, wired to whatever key the app means.
//!
//! Truncation order is pinned: the TITLE yields before the ✕ (a close
//! affordance you can't see can't free the space you want back), and
//! at 1–2 total columns the ✕ yields too — nothing paints on or
//! outside the frame. Borderless closable blocks float the ✕ over the
//! top-right content cells (they reserved no chrome row; the app
//! chose both).
//!
//! `on_close` may synchronously remove the panel (dispose the block's
//! scope): all widget bookkeeping lands before the callback runs (the
//! 0297 disposal law).
//!
//! OWNER: DESIGN.

use std::cell::Cell;
use std::rc::Rc;

use crate::base::{Point, Rect, Rgba};
use crate::layout::{Edges, Style as LayoutStyle};
use crate::theme::TokenSet;
use crate::ui::{Canvas, Element, View};

// The close affordance's geometry + interactive overlay — private
// shipped sibling (file-size discipline, the feed_typeset.rs pattern).
#[path = "block_close.rs"]
mod close;
#[cfg(test)]
use close::CLOSE_GLYPH;
use close::{close_child, close_run, close_text, panel_rect};

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

/// The panel container: an optional border (plain/rounded/double/heavy)
/// with an optional title, a ground fill, a cell-space drop shadow, the
/// focus ring (`border_focus` ink when [`focused`](Block::focused)),
/// and an opt-in close affordance ([`on_close`](Block::on_close)).
///
/// Blocks are how an app builds its chrome — cards, panes, sidebars:
/// children lay out inside the border box. Stateless: build with
/// `.element(&tokens)` (or `.view(cx)` for theme-from-context). See the
/// [module docs](crate::widgets::block).
pub struct Block {
    border: BorderKind,
    title: Option<String>,
    title_align: TitleAlign,
    focused: bool,
    fill: Option<Rgba>,
    shadow: Option<Rgba>,
    layout: LayoutStyle,
    children: Vec<View>,
    on_close: Option<Box<dyn FnMut()>>,
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
            on_close: None,
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

    /// Opt into the close affordance: a `✕` at the title row's right
    /// end, MOUSE-ONLY (press + release inside — the Button convention),
    /// never focusable. `f` runs when the user clicks it — removing the
    /// panel from the app's layout inside `f` is the normal body (the
    /// widget's own bookkeeping lands first, so disposing the block's
    /// scope synchronously is safe — the 0297 disposal law). Keyboard
    /// close stays app-side. See the [module docs](crate::widgets::block)
    /// for the truncation order and visuals.
    pub fn on_close(mut self, f: impl FnMut() + 'static) -> Block {
        self.on_close = Some(Box::new(f));
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
        // Close inks resolve NOW like every Block color (§5): muted at
        // rest, `error` hot — a consequence-bearing action, not a
        // neutral accent one (the diff-vocabulary precedent).
        let close_muted = t.text_muted;
        let close_hot = t.error;
        let border = self.border;
        let title = self.title;
        // The close child's a11y label wants the title too; clone before
        // the draw closure takes ownership below.
        let access_title = if self.on_close.is_some() {
            title.clone()
        } else {
            None
        };
        let align = self.title_align;
        let fill = self.fill;
        let shadow = self.shadow;
        let bordered = border.glyphs().is_some();
        let closable = self.on_close.is_some();
        // Geometry probe (closable blocks only): the root draw publishes
        // the PANEL rect every paint, and the close child does ALL its
        // hit/paint math against it — one geometry owner, so the ✕ the
        // frame shows and the ✕ the mouse hits can never disagree. Zero
        // until the first paint: clicks before anything is visible are
        // honestly inert.
        let geom: Option<Rc<Cell<Rect>>> = closable.then(|| Rc::new(Cell::new(Rect::ZERO)));

        // Chrome insets ride a PROTECTED padding floor, not the plain
        // style: a caller's later `.style(grow)` on the returned Element
        // then sizes the panel WITHOUT dropping content onto the border
        // (RT8-7 — the worst first-use trap of the cycle-8 review).
        let mut chrome = Edges::ZERO;
        if bordered {
            chrome = Edges::all(1);
        }
        if shadow.is_some() {
            // The strip takes the last column/row; children stay inside
            // the lifted panel.
            chrome.right += 1;
            chrome.bottom += 1;
        }
        let layout = self.layout;

        let geom_probe = geom.clone();
        let mut el =
            Element::new()
                .style(layout)
                .padding_floor(chrome)
                .draw(move |canvas, rect| {
                    let panel = panel_rect(rect, shadow.is_some());
                    if let Some(g) = &geom_probe {
                        // Published BEFORE the degenerate-rect guard (and on
                        // culled paints — `probe_when_culled` below), so a
                        // crushed block retracts its ✕ the same frame it
                        // collapses.
                        g.set(panel);
                    }
                    if rect.w <= 0 || rect.h <= 0 {
                        return;
                    }
                    if let Some(ground) = shadow {
                        // Offset strip: right column + bottom row, shifted one
                        // cell down-right — reads as light from the top-left.
                        for y in (rect.y + 1)..rect.bottom() {
                            canvas.put(Point::new(rect.right() - 1, y), ' ', ground, ground);
                        }
                        for x in (rect.x + 1)..rect.right() {
                            canvas.put(Point::new(x, rect.bottom() - 1), ' ', ground, ground);
                        }
                    }
                    let close = if closable {
                        close_run(panel, bordered).map(|run| (run, close_muted))
                    } else {
                        None
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
                        close,
                    );
                });
        if closable {
            // Zero-area paints are normally skipped (the fusion rule);
            // the geometry probe must keep publishing through them.
            el = el.probe_when_culled();
        }
        for child in self.children {
            el = el.child(child);
        }
        if let (Some(geom), Some(on_close)) = (geom, self.on_close) {
            el = el.child(close_child(
                geom,
                bordered,
                access_title,
                close_hot,
                on_close,
            ));
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
    close: Option<(Rect, Rgba)>,
) {
    if rect.w <= 0 || rect.h <= 0 {
        return;
    }
    if let Some(ground) = fill {
        canvas.fill(rect, ' ', ground, ground);
    }
    let keep = Rgba::TRANSPARENT; // alpha-0 bg: keep what's beneath
    let bg = fill.unwrap_or(keep);
    let Some([tl, top, tr, left, right, bl, bottom, br]) = border.glyphs() else {
        // Borderless: no strokes, no title — but the close affordance
        // still paints (floating over the top-right content cells; the
        // app opted into both).
        if let Some((run, ink)) = close {
            canvas.print(Point::new(run.x, run.y), close_text(run.w), ink, bg);
        }
        return;
    };
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

    // The close run paints over the top stroke, at rest in its muted ink
    // (the hot restyle is the close child's — same geometry, one owner).
    if let Some((run, ink)) = close {
        canvas.print(Point::new(run.x, run.y), close_text(run.w), ink, bg);
    }

    // Title rides the top stroke, padded, truncated to the run LEFT of
    // the close affordance — the title yields first (pinned order).
    let Some(title) = title else { return };
    let title_end = close.map(|(run, _)| run.x).unwrap_or(x1); // exclusive
    let avail = (title_end - (x0 + 1) - 2).max(0) as usize;
    if avail == 0 || title.is_empty() {
        return;
    }
    let shown: String = title.chars().take(avail).collect();
    let w = shown.chars().count() as i32 + 2; // " title "
    let tx = match align {
        TitleAlign::Left => x0 + 1,
        TitleAlign::Center => x0 + (rect.w - w).max(0) / 2,
        TitleAlign::Right => title_end - w,
    }
    .clamp(x0 + 1, (title_end - w).max(x0 + 1));
    canvas.print(Point::new(tx, y0), &format!(" {shown} "), title_fg, bg);
}

#[cfg(test)]
#[path = "block_tests.rs"]
mod tests;
