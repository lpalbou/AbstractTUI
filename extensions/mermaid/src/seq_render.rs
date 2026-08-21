//! Sequence-diagram painting: cell glyphs over a [`SeqPlan`].
//!
//! Z-order (documented), later paints winning the cell: lifelines,
//! then activation bars riding them, then control-flow frames and
//! their dividers, then messages/notes in source order, then
//! participant boxes. So arrows cross lifelines legibly, boxes stay
//! crisp, and a frame's border is never erased by the bar or the
//! lifeline it crosses — one cell cannot show both, and a broken frame
//! reads worse than a broken line.
//!
//! `plan.rows` is therefore read in three passes rather than one: the
//! plan is a set of z-layers in source order, not a paint list.
//! Colors arrive resolved through [`SeqStyle`] (the widget token
//! rule).

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
    /// Control-flow frame border (`alt`/`opt`/`loop`/`par`).
    pub frame_border: Rgba,
    /// The `alt: is lunchtime` tab and `else` divider labels.
    pub frame_label: Rgba,
    /// Activation bar on a lifeline.
    pub activation: Rgba,
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
            // Frames are chrome AROUND the conversation: they must be
            // legible without competing with the messages inside.
            frame_border: t.border,
            frame_label: t.text_muted,
            activation: t.accent,
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

    // Chrome, in the order it must stack: activation bars ride the
    // lifelines, then frames and their dividers paint over them —
    // otherwise a bar crossing a frame's top border erases the tab
    // that names the construct. Messages come last so an arrow
    // crossing either stays legible (the module's z-order rule).
    for row in &plan.rows {
        if let RowPlan::Activation {
            col,
            y0,
            y1,
            offset,
        } = row
        {
            let x = plan.columns[*col].center + offset;
            for y in *y0..=*y1 {
                canvas.put(at(x, y), '┃', style.activation, Rgba::TRANSPARENT);
            }
        }
    }
    for row in &plan.rows {
        match row {
            RowPlan::Frame { rect, tab } => draw_frame(canvas, origin, *rect, tab, style),
            RowPlan::Divider { y, x0, x1, tab } => {
                draw_divider(canvas, origin, *y, *x0, *x1, tab, style)
            }
            _ => {}
        }
    }

    // Where an activation bar sits on each row, so an arrow can stop
    // AT the bar instead of punching a hole through it — mermaid
    // terminates messages on the activation rectangle's edge.
    let bar_at = |col: usize, row: i32| -> Option<i32> {
        plan.rows
            .iter()
            .filter_map(|r| match r {
                RowPlan::Activation {
                    col: c,
                    y0,
                    y1,
                    offset,
                } if *c == col && (*y0..=*y1).contains(&row) => Some(*offset),
                _ => None,
            })
            .max()
    };

    // Rows in source order.
    for row in &plan.rows {
        match row {
            RowPlan::Frame { .. } | RowPlan::Divider { .. } | RowPlan::Activation { .. } => {}
            RowPlan::Message {
                y,
                from_col,
                to_col,
                kind,
                text,
            } => {
                let (line, right_head, left_head) = glyphs(*kind);
                let (cf, ct) = (plan.columns[*from_col].center, plan.columns[*to_col].center);
                // A bar on the receiving lifeline is the edge the
                // message lands on; without this the head overprints
                // the bar and leaves a hole in it.
                let ct = ct + bar_at(*to_col, y + 1).map_or(0, |off| if ct > cf { off } else { 0 });
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

/// A control-flow frame: a light box with the construct named in a tab
/// on its top edge (`alt: is lunchtime`), the way mermaid labels one.
/// The tab is what tells a reader WHICH construct they are inside, so
/// it takes priority over the border when the frame is narrow.
fn draw_frame(
    canvas: &mut dyn StyledCanvas,
    origin: Point,
    rect: abstracttui::base::Rect,
    tab: &str,
    style: &SeqStyle,
) {
    if rect.w < 2 || rect.h < 2 {
        return;
    }
    let r = rect.translate(origin.x, origin.y);
    let (l, t, rgt, b) = (r.x, r.y, r.right() - 1, r.bottom() - 1);
    for x in (l + 1)..rgt {
        canvas.put(Point::new(x, t), '─', style.frame_border, Rgba::TRANSPARENT);
        canvas.put(Point::new(x, b), '─', style.frame_border, Rgba::TRANSPARENT);
    }
    for y in (t + 1)..b {
        canvas.put(Point::new(l, y), '│', style.frame_border, Rgba::TRANSPARENT);
        canvas.put(
            Point::new(rgt, y),
            '│',
            style.frame_border,
            Rgba::TRANSPARENT,
        );
    }
    canvas.put(Point::new(l, t), '╭', style.frame_border, Rgba::TRANSPARENT);
    canvas.put(
        Point::new(rgt, t),
        '╮',
        style.frame_border,
        Rgba::TRANSPARENT,
    );
    canvas.put(Point::new(l, b), '╰', style.frame_border, Rgba::TRANSPARENT);
    canvas.put(
        Point::new(rgt, b),
        '╯',
        style.frame_border,
        Rgba::TRANSPARENT,
    );

    if !tab.is_empty() {
        canvas.print(
            Point::new(l + 2, t),
            tab,
            style.frame_label,
            Rgba::TRANSPARENT,
        );
    }
}

/// An `else`/`and` divider: a dashed rule across the frame with its
/// label at the left, so the branches read as alternatives rather than
/// as a sequence.
fn draw_divider(
    canvas: &mut dyn StyledCanvas,
    origin: Point,
    y: i32,
    x0: i32,
    x1: i32,
    tab: &str,
    style: &SeqStyle,
) {
    let y = origin.y + y;
    for x in (x0 + 1)..x1 {
        canvas.put(
            Point::new(origin.x + x, y),
            '╌',
            style.frame_border,
            Rgba::TRANSPARENT,
        );
    }
    if !tab.is_empty() {
        canvas.print(
            Point::new(origin.x + x0 + 2, y),
            tab,
            style.frame_label,
            Rgba::TRANSPARENT,
        );
    }
}
