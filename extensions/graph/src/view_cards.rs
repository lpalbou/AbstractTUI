//! Node-card painting for [`GraphView`](crate::view::GraphView): a
//! themed box with the title on the top border (the compact recipe),
//! an optional kind-tinted accent on the left border column, and a
//! badge row. Colors arrive RESOLVED (`Rgba`) through
//! [`GraphStyle`](crate::view::GraphStyle) — this module invents none
//! (the engine's widget token rule, held by the sibling-crate family).
//!
//! OWNER: CANVAS (view half of 0440).

use abstracttui::base::{Point, Rect, Rgba};
use abstracttui::render::Style;
use abstracttui::text::truncate_ellipsis;
use abstracttui::ui::StyledCanvas;

use crate::view::GraphStyle;

/// One card's paint-time facts (resolved at build; the draw closure
/// captures them).
pub(crate) struct CardPaint {
    pub title: String,
    pub badge: Option<String>,
    /// Kind accent ink for the left border column (`None` = plain).
    pub accent: Option<Rgba>,
}

/// Paint a card into `rect`. Degrades honestly with size: full border
/// plus title plus badge row at h >= 3, border plus title at h == 2,
/// a bare truncated title chip at h == 1 or very narrow rects.
pub(crate) fn draw_card(
    canvas: &mut dyn StyledCanvas,
    rect: Rect,
    style: &GraphStyle,
    paint: &CardPaint,
    selected: bool,
) {
    if rect.w <= 0 || rect.h <= 0 {
        return;
    }
    let border_ink = if selected {
        style.card_border_selected
    } else {
        style.card_border
    };
    // Selection restyle wins the whole border; the kind accent tints
    // the left column only while unselected (selection must be
    // unmistakable).
    let left_ink = match (selected, paint.accent) {
        (false, Some(accent)) => accent,
        _ => border_ink,
    };
    let bg = style.card_bg;

    // Ground first: title/badge rows inherit the card surface.
    canvas.fill(rect, ' ', style.card_title, bg);

    if rect.w < 3 || rect.h < 2 {
        // Chip degradation: no room for a border.
        let text = truncate_ellipsis(&paint.title, rect.w);
        let mut s = Style::new().fg(style.card_title).bg(bg);
        if selected {
            s = s.bold().reverse();
        }
        canvas.print_styled(Point::new(rect.x, rect.y), &text, &s);
        return;
    }

    let (top, bottom) = (rect.y, rect.bottom() - 1);
    let (left, right) = (rect.x, rect.right() - 1);
    // Horizontal runs.
    for x in (left + 1)..right {
        canvas.put(Point::new(x, top), '─', border_ink, bg);
        canvas.put(Point::new(x, bottom), '─', border_ink, bg);
    }
    // Vertical runs (left column carries the kind accent).
    for y in (top + 1)..bottom {
        canvas.put(Point::new(left, y), '│', left_ink, bg);
        canvas.put(Point::new(right, y), '│', border_ink, bg);
    }
    canvas.put(Point::new(left, top), '╭', left_ink, bg);
    canvas.put(Point::new(right, top), '╮', border_ink, bg);
    canvas.put(Point::new(left, bottom), '╰', left_ink, bg);
    canvas.put(Point::new(right, bottom), '╯', border_ink, bg);

    // Title on the top border, Block-style: "╭ Title ──╮".
    let title_budget = rect.w - 4;
    if title_budget > 0 && !paint.title.is_empty() {
        let text = truncate_ellipsis(&paint.title, title_budget);
        let mut s = Style::new().fg(style.card_title).bg(bg);
        if selected {
            s = s.bold();
        }
        let advanced = canvas.print_styled(Point::new(left + 2, top), &text, &s);
        // One breathing space each side of the title run.
        canvas.put(Point::new(left + 1, top), ' ', border_ink, bg);
        canvas.put(Point::new(left + 2 + advanced, top), ' ', border_ink, bg);
    }

    // Badge: right-aligned on the first content row (needs h >= 3).
    if rect.h >= 3 {
        if let Some(badge) = paint.badge.as_deref() {
            let budget = rect.w - 2;
            if budget > 0 && !badge.is_empty() {
                let text = truncate_ellipsis(badge, budget);
                let w = abstracttui::text::width(&text);
                let x = right - w;
                let s = Style::new().fg(style.badge).bg(bg);
                canvas.print_styled(Point::new(x, top + 1), &text, &s);
            }
        }
    }
}
