//! Sequence-diagram painting: cell glyphs over a [`SeqPlan`].
//!
//! Z-order (documented): lifelines, then messages/notes in source
//! order, then participant boxes — later paints win cells, so arrows
//! cross lifelines legibly and boxes stay crisp. Colors arrive
//! resolved through [`SeqStyle`] (the widget token rule).

use abstracttui::base::{Point, Rgba};
use abstracttui::text::{truncate_ellipsis, width};
use abstracttui::theme::TokenSet;
use abstracttui::ui::StyledCanvas;

use crate::ir::MessageKind;
use crate::seq_layout::{RowPlan, SeqPlan};

/// Resolved ink set for sequence rendering. Author-written,
/// shape-stable: plain fields + `Default` + FRU per ADR-0003 §2.
#[derive(Clone, Debug, PartialEq)]
pub struct SeqStyle {
    /// Message label ink.
    pub text: Rgba,
    /// Lifeline ink.
    pub lifeline: Rgba,
    /// Message line + arrowhead ink.
    pub arrow: Rgba,
    /// Participant box border ink.
    pub box_border: Rgba,
    /// Participant box fill.
    pub box_bg: Rgba,
    /// Participant label ink.
    pub box_title: Rgba,
    /// Note border ink (mermaid notes read as callouts: warn-tinted).
    pub note_border: Rgba,
    /// Note fill.
    pub note_bg: Rgba,
    /// Note text ink.
    pub note_text: Rgba,
    /// Notice line ink (dropped-directive honesty).
    pub notice: Rgba,
}

impl SeqStyle {
    /// Derive the default ink set from a resolved token set.
    pub fn from_tokens(t: &TokenSet) -> SeqStyle {
        SeqStyle {
            text: t.text,
            lifeline: t.text_faint,
            arrow: t.text_muted,
            box_border: t.border,
            box_bg: t.surface_raised,
            box_title: t.text,
            note_border: t.warn,
            note_bg: t.surface_raised,
            note_text: t.text_muted,
            notice: t.warn,
        }
    }
}

impl Default for SeqStyle {
    fn default() -> Self {
        SeqStyle::from_tokens(&abstracttui::theme::default_theme().tokens)
    }
}

/// Line/arrowhead glyphs for a message kind.
fn glyphs(kind: MessageKind) -> (char, char, char) {
    let line = if kind.dashed() { '╌' } else { '─' };
    let (right, left) = if kind.filled() {
        ('▶', '◀')
    } else {
        ('>', '<')
    };
    (line, right, left)
}

/// Paint the plan at `origin`.
pub(crate) fn draw(canvas: &mut dyn StyledCanvas, origin: Point, plan: &SeqPlan, style: &SeqStyle) {
    let at = |x: i32, y: i32| Point::new(origin.x + x, origin.y + y);

    // Lifelines, under everything.
    for col in &plan.columns {
        for y in plan.columns[0].box_rect.h..plan.height {
            canvas.put(at(col.center, y), '│', style.lifeline, Rgba::TRANSPARENT);
        }
    }

    // Rows in source order.
    for row in &plan.rows {
        match row {
            RowPlan::Message {
                y,
                from_col,
                to_col,
                kind,
                text,
            } => {
                let (line, right_head, left_head) = glyphs(*kind);
                let (cf, ct) = (plan.columns[*from_col].center, plan.columns[*to_col].center);
                let (lo, hi) = (cf.min(ct), cf.max(ct));
                for x in (lo + 1)..hi {
                    canvas.put(at(x, y + 1), line, style.arrow, Rgba::TRANSPARENT);
                }
                let (head_x, head) = if ct > cf {
                    (ct - 1, right_head)
                } else {
                    (ct + 1, left_head)
                };
                canvas.put(at(head_x, y + 1), head, style.arrow, Rgba::TRANSPARENT);
                // Label centered above the arrow, within the span.
                let span = hi - lo - 1;
                if span > 0 {
                    let t = truncate_ellipsis(text, span);
                    let x = lo + 1 + (span - width(&t)) / 2;
                    canvas.print(at(x, *y), &t, style.text, Rgba::TRANSPARENT);
                }
            }
            RowPlan::SelfMessage { y, col, kind, text } => {
                let (line, _, left_head) = glyphs(*kind);
                let c = plan.columns[*col].center;
                canvas.print(at(c + 6, *y), text, style.text, Rgba::TRANSPARENT);
                for x in (c + 1)..(c + 4) {
                    canvas.put(at(x, y + 1), line, style.arrow, Rgba::TRANSPARENT);
                }
                canvas.put(at(c + 4, y + 1), '╮', style.arrow, Rgba::TRANSPARENT);
                canvas.put(at(c + 4, y + 2), '╯', style.arrow, Rgba::TRANSPARENT);
                for x in (c + 2)..(c + 4) {
                    canvas.put(at(x, y + 2), line, style.arrow, Rgba::TRANSPARENT);
                }
                canvas.put(at(c + 1, y + 2), left_head, style.arrow, Rgba::TRANSPARENT);
            }
            RowPlan::Note { rect, text } => {
                let r = rect.translate(origin.x, origin.y);
                canvas.fill(r, ' ', style.note_text, style.note_bg);
                for x in (r.x + 1)..(r.right() - 1) {
                    canvas.put(Point::new(x, r.y), '─', style.note_border, style.note_bg);
                    canvas.put(
                        Point::new(x, r.bottom() - 1),
                        '─',
                        style.note_border,
                        style.note_bg,
                    );
                }
                canvas.put(Point::new(r.x, r.y), '┌', style.note_border, style.note_bg);
                canvas.put(
                    Point::new(r.right() - 1, r.y),
                    '┐',
                    style.note_border,
                    style.note_bg,
                );
                canvas.put(
                    Point::new(r.x, r.bottom() - 1),
                    '└',
                    style.note_border,
                    style.note_bg,
                );
                canvas.put(
                    Point::new(r.right() - 1, r.bottom() - 1),
                    '┘',
                    style.note_border,
                    style.note_bg,
                );
                canvas.put(
                    Point::new(r.x, r.y + 1),
                    '│',
                    style.note_border,
                    style.note_bg,
                );
                canvas.put(
                    Point::new(r.right() - 1, r.y + 1),
                    '│',
                    style.note_border,
                    style.note_bg,
                );
                let budget = r.w - 2;
                if budget > 0 {
                    let t = truncate_ellipsis(text, budget);
                    let x = r.x + 1 + (budget - width(&t)) / 2;
                    canvas.print(Point::new(x, r.y + 1), &t, style.note_text, style.note_bg);
                }
            }
        }
    }

    // Participant boxes, on top.
    for col in &plan.columns {
        let r = col.box_rect.translate(origin.x, origin.y);
        canvas.fill(r, ' ', style.box_title, style.box_bg);
        for x in (r.x + 1)..(r.right() - 1) {
            canvas.put(Point::new(x, r.y), '─', style.box_border, style.box_bg);
            canvas.put(
                Point::new(x, r.bottom() - 1),
                '─',
                style.box_border,
                style.box_bg,
            );
        }
        canvas.put(Point::new(r.x, r.y), '╭', style.box_border, style.box_bg);
        canvas.put(
            Point::new(r.right() - 1, r.y),
            '╮',
            style.box_border,
            style.box_bg,
        );
        canvas.put(
            Point::new(r.x, r.bottom() - 1),
            '╰',
            style.box_border,
            style.box_bg,
        );
        canvas.put(
            Point::new(r.right() - 1, r.bottom() - 1),
            '╯',
            style.box_border,
            style.box_bg,
        );
        canvas.put(
            Point::new(r.x, r.y + 1),
            '│',
            style.box_border,
            style.box_bg,
        );
        canvas.put(
            Point::new(r.right() - 1, r.y + 1),
            '│',
            style.box_border,
            style.box_bg,
        );
        let budget = r.w - 2;
        if budget > 0 {
            let t = truncate_ellipsis(&col.label, budget);
            let x = r.x + 1 + (budget - width(&t)) / 2;
            canvas.print(Point::new(x, r.y + 1), &t, style.box_title, style.box_bg);
        }
    }
}
