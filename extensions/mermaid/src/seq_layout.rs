//! Sequence-diagram layout: deterministic columns/rows, NO solver.
//!
//! Lifeline columns come from participant order (gaps sized by box
//! halves and ADJACENT-pair message labels — longer spans take what
//! the columns give and truncate at render); message/note rows come
//! from source order. Pure integer math over the IR: same diagram,
//! same plan, golden-pinnable.

use abstracttui::base::Rect;
use abstracttui::text::width;

use crate::ir::{MessageKind, NoteAnchor, SeqItem, SequenceIr};

/// Participant box height (border + label + border).
const BOX_H: i32 = 3;
/// First content row (one breathing row under the boxes).
const CONTENT_Y: i32 = BOX_H + 1;
/// Minimum clearance between adjacent participant boxes.
const BOX_GAP: i32 = 2;

/// One lifeline column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColumnPlan {
    /// Lifeline x (cells).
    pub center: i32,
    /// Participant box at the top.
    pub box_rect: Rect,
    /// Display label (alias or id).
    pub label: String,
}

/// One content row group, in source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RowPlan {
    /// `from != to`: label row at `y`, arrow row at `y + 1`.
    Message {
        y: i32,
        from_col: usize,
        to_col: usize,
        kind: MessageKind,
        text: String,
    },
    /// Self-message: label row at `y`, loop rows at `y+1`/`y+2`.
    SelfMessage {
        y: i32,
        col: usize,
        kind: MessageKind,
        text: String,
    },
    /// Note box (3 rows tall).
    Note { rect: Rect, text: String },
}

/// The whole plan: canvas size, columns, rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SeqPlan {
    pub width: i32,
    pub height: i32,
    pub columns: Vec<ColumnPlan>,
    pub rows: Vec<RowPlan>,
}

pub(crate) fn plan(seq: &SequenceIr) -> SeqPlan {
    let labels: Vec<String> = seq
        .participants
        .iter()
        .map(|p| p.label().to_string())
        .collect();
    let box_w: Vec<i32> = labels.iter().map(|l| (width(l) + 4).max(6)).collect();
    let col_of = |id: &str| -> usize {
        seq.participants
            .iter()
            .position(|p| p.id == id)
            .unwrap_or(0)
    };

    // Adjacent-pair label needs (unordered pair (i, i+1) keyed by i).
    let mut adj_need = vec![0i32; labels.len().saturating_sub(1)];
    for item in &seq.items {
        if let SeqItem::Message(m) = item {
            let (a, b) = (col_of(&m.from), col_of(&m.to));
            if a.abs_diff(b) == 1 {
                let slot = a.min(b);
                adj_need[slot] = adj_need[slot].max(width(&m.text) + 4);
            }
        }
    }

    // Column centers: box halves + gap, or the adjacent label need.
    let mut centers = Vec::with_capacity(labels.len());
    for i in 0..labels.len() {
        if i == 0 {
            centers.push(box_w[0] / 2);
        } else {
            let step =
                ((box_w[i - 1] - box_w[i - 1] / 2) + box_w[i] / 2 + BOX_GAP).max(adj_need[i - 1]);
            centers.push(centers[i - 1] + step);
        }
    }

    // Rows in source order.
    let mut rows: Vec<RowPlan> = Vec::with_capacity(seq.items.len());
    let mut y = CONTENT_Y;
    let mut max_right = 0i32;
    let mut min_left = 0i32;
    for item in &seq.items {
        match item {
            SeqItem::Message(m) => {
                let (from_col, to_col) = (col_of(&m.from), col_of(&m.to));
                if from_col == to_col {
                    rows.push(RowPlan::SelfMessage {
                        y,
                        col: from_col,
                        kind: m.kind,
                        text: m.text.clone(),
                    });
                    max_right = max_right.max(centers[from_col] + 6 + width(&m.text));
                    y += 4;
                } else {
                    rows.push(RowPlan::Message {
                        y,
                        from_col,
                        to_col,
                        kind: m.kind,
                        text: m.text.clone(),
                    });
                    y += 3;
                }
            }
            SeqItem::Note(n) => {
                let rect = note_rect(&centers, &col_of, y, n);
                min_left = min_left.min(rect.x);
                max_right = max_right.max(rect.right());
                rows.push(RowPlan::Note {
                    rect,
                    text: n.text.clone(),
                });
                y += 4;
            }
        }
    }

    // Left-overflowing notes shift the whole picture right (origin
    // stays 0,0 — the scroll container owns panning).
    let shift = -min_left;
    let columns: Vec<ColumnPlan> = labels
        .into_iter()
        .zip(&box_w)
        .zip(&centers)
        .map(|((label, &bw), &c)| {
            let center = c + shift;
            ColumnPlan {
                center,
                box_rect: Rect::new(center - bw / 2, 0, bw, BOX_H),
                label,
            }
        })
        .collect();
    if shift != 0 {
        for row in &mut rows {
            if let RowPlan::Note { rect, .. } = row {
                *rect = rect.translate(shift, 0);
            }
        }
    }

    let boxes_right = columns
        .iter()
        .map(|c| c.box_rect.right())
        .max()
        .unwrap_or(1);
    SeqPlan {
        width: boxes_right.max(max_right + shift) + 1,
        height: (y + 1).max(CONTENT_Y + 1),
        columns,
        rows,
    }
}

/// Note geometry from its anchor (may go negative on the left; the
/// caller shifts).
fn note_rect(centers: &[i32], col_of: &dyn Fn(&str) -> usize, y: i32, n: &crate::ir::Note) -> Rect {
    let w_text = width(&n.text) + 4;
    match &n.anchor {
        NoteAnchor::LeftOf(id) => {
            let c = centers[col_of(id)];
            Rect::new(c - 2 - w_text, y, w_text, 3)
        }
        NoteAnchor::RightOf(id) => {
            let c = centers[col_of(id)];
            Rect::new(c + 2, y, w_text, 3)
        }
        NoteAnchor::Over(a, b) => {
            let ca = centers[col_of(a)];
            let cb = b.as_ref().map_or(ca, |b| centers[col_of(b)]);
            let (lo, hi) = (ca.min(cb), ca.max(cb));
            let span = (hi - lo + 6).max(w_text);
            let mid = (lo + hi) / 2;
            Rect::new(mid - span / 2, y, span, 3)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Message, Participant};

    fn two_party() -> SequenceIr {
        SequenceIr {
            participants: vec![
                Participant {
                    id: "a".into(),
                    alias: Some("Alice".into()),
                },
                Participant {
                    id: "b".into(),
                    alias: None,
                },
            ],
            items: vec![SeqItem::Message(Message {
                from: "a".into(),
                to: "b".into(),
                kind: MessageKind::SolidArrow,
                text: "hello".into(),
            })],
            notices: Vec::new(),
        }
    }

    #[test]
    fn columns_and_rows_are_deterministic_integer_math() {
        let p = plan(&two_party());
        assert_eq!(p, plan(&two_party()), "same IR, same plan");
        assert_eq!(p.columns.len(), 2);
        // Boxes never overlap and the second lifeline is right of the
        // first.
        assert!(p.columns[0].box_rect.right() + BOX_GAP <= p.columns[1].box_rect.x);
        assert!(p.columns[0].center < p.columns[1].center);
        // One message: label row + arrow row starting at CONTENT_Y.
        assert_eq!(
            p.rows,
            vec![RowPlan::Message {
                y: CONTENT_Y,
                from_col: 0,
                to_col: 1,
                kind: MessageKind::SolidArrow,
                text: "hello".into(),
            }]
        );
        assert!(p.width > p.columns[1].center);
        assert!(p.height >= CONTENT_Y + 3);
    }

    #[test]
    fn left_notes_shift_the_picture_instead_of_clipping() {
        let mut seq = two_party();
        seq.items.push(SeqItem::Note(crate::ir::Note {
            anchor: NoteAnchor::LeftOf("a".into()),
            text: "a rather long note".into(),
        }));
        let p = plan(&seq);
        for row in &p.rows {
            if let RowPlan::Note { rect, .. } = row {
                assert!(rect.x >= 0, "notes never clip left: {rect:?}");
            }
        }
        assert!(p.columns[0].box_rect.x >= 0);
    }
}
