//! Sequence-diagram layout: deterministic columns/rows, NO solver.
//!
//! Lifeline columns come from participant order (gaps sized by box
//! halves and ADJACENT-pair message labels — longer spans take what
//! the columns give and truncate at render); message/note rows come
//! from source order. Pure integer math over the IR: same diagram,
//! same plan, golden-pinnable.

use abstracttui::base::Rect;
use abstracttui::text::{truncate_ellipsis, width};

use crate::ir::{BlockKind, MessageKind, NoteAnchor, SeqItem, SequenceIr};

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
    /// A control-flow frame around the rows between `rect.y` and
    /// `rect.bottom()`. `tab` is the FITTED label — layout sized the
    /// frame for it, so rendering prints it and measures nothing.
    Frame { rect: Rect, tab: String },
    /// An `else`/`and` divider inside a frame, with its fitted label.
    Divider {
        y: i32,
        x0: i32,
        x1: i32,
        tab: String,
    },
    /// An activation bar on a lifeline, `y0..=y1` inclusive. `offset`
    /// is the cell shift for nested activations of one participant.
    Activation {
        col: usize,
        y0: i32,
        y1: i32,
        offset: i32,
    },
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
    for_each_message(&seq.items, &mut |m| {
        let (a, b) = (col_of(&m.from), col_of(&m.to));
        if a.abs_diff(b) == 1 {
            let slot = a.min(b);
            adj_need[slot] = adj_need[slot].max(width(&m.text) + 4);
        }
    });

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

    // Rows in source order. Blocks recurse; the walk carries the
    // bookkeeping every nested construct needs (the next row, how deep
    // the frames are, and which participants are currently active).
    let ctx = Ctx {
        centers: &centers,
        participants: &seq.participants,
    };
    let extents = Extents::build(&seq.items, &ctx);
    let mut walk = Walk {
        ctx: &ctx,
        extents: &extents,
        next_block: 0,
        rows: Vec::with_capacity(seq.items.len()),
        y: CONTENT_Y,
        max_right: 0,
        min_left: 0,
        last_arrow_y: None,
        active: Vec::new(),
    };
    walk.items(&seq.items, 0, None);
    walk.close_open_activations();
    let Walk {
        mut rows,
        y,
        mut max_right,
        min_left,
        ..
    } = walk;
    for row in &rows {
        if let RowPlan::Frame { rect, .. } = row {
            max_right = max_right.max(rect.right());
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
            match row {
                RowPlan::Note { rect, .. } => *rect = rect.translate(shift, 0),
                RowPlan::Frame { rect, .. } => *rect = rect.translate(shift, 0),
                RowPlan::Divider { x0, x1, .. } => {
                    *x0 += shift;
                    *x1 += shift;
                }
                RowPlan::Message { .. }
                | RowPlan::SelfMessage { .. }
                | RowPlan::Activation { .. } => {}
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

/// Visit every message in the tree, blocks included. The column pass
/// needs them all before any geometry exists, which is the one thing
/// that cannot be folded into the extent walk below.
fn for_each_message(items: &[SeqItem], f: &mut impl FnMut(&crate::ir::Message)) {
    for item in items {
        match item {
            SeqItem::Message(m) => f(m),
            SeqItem::Block(b) => {
                for branch in b.branches() {
                    for_each_message(&branch.items, f);
                }
            }
            _ => {}
        }
    }
}

/// The frame tab a block paints. Layout FITS it once and stores the
/// finished string in [`RowPlan::Frame`]; rendering prints it and does
/// no arithmetic, so the two cannot disagree about the width they
/// reserved.
fn tab_text(kind: BlockKind, label: &str) -> String {
    match label.trim() {
        "" => format!(" {} ", kind.keyword()),
        text => format!(" {}: {text} ", kind.keyword()),
    }
}

/// Cells of frame border + air a tab needs beside its text.
const TAB_MARGIN: i32 = 4;

/// What a block needs horizontally, measured ONCE bottom-up.
///
/// Two things drive a frame's width: the lifelines and note boxes its
/// contents actually touch, and the widest label anywhere in it —
/// including its own dividers and everything nested inside, because a
/// child frame must fit inside its parent.
#[derive(Clone, Debug)]
struct Extent {
    /// Leftmost / rightmost cell the block's CONTENTS occupy.
    lo: i32,
    hi: i32,
    /// Minimum frame width, this block's tabs and its children's.
    need: i32,
}

/// Extents for every block in the tree, in the depth-first order the
/// layout walk meets them. Computing these bottom-up in one pass is
/// what keeps layout linear: deriving each block's span top-down
/// re-walked the whole subtree per block, which is quadratic in
/// nesting depth (and measurably so past a few hundred blocks).
struct Extents {
    order: Vec<Extent>,
}

impl Extents {
    fn build(items: &[SeqItem], ctx: &Ctx<'_>) -> Extents {
        let mut out = Extents { order: Vec::new() };
        out.items(items, ctx);
        out
    }

    /// Extent of a run of items; blocks push their own entry first
    /// (pre-order), so the walk can pop them in the order it meets them.
    fn items(&mut self, items: &[SeqItem], ctx: &Ctx<'_>) -> Extent {
        let mut ext = Extent {
            lo: i32::MAX,
            hi: i32::MIN,
            need: 0,
        };
        for item in items {
            match item {
                SeqItem::Message(m) => {
                    let (a, b) = (ctx.col_of(&m.from), ctx.col_of(&m.to));
                    ext.lo = ext.lo.min(ctx.center(a)).min(ctx.center(b));
                    ext.hi = ext.hi.max(ctx.center(a)).max(ctx.center(b));
                    if a == b {
                        // A self-message writes its label to the RIGHT
                        // of the lifeline: the frame has to hold that
                        // too, or the text runs through the border.
                        ext.hi = ext.hi.max(ctx.center(a) + SELF_LABEL_X + width(&m.text));
                    }
                }
                SeqItem::Note(n) => {
                    // Notes are boxes, and they are FILLED at paint
                    // time — a note poking out of a frame does not
                    // overlap the border, it erases it.
                    let rect = note_rect(ctx.centers, &|id| ctx.col_of(id), 0, n);
                    ext.lo = ext.lo.min(rect.x);
                    ext.hi = ext.hi.max(rect.right());
                }
                SeqItem::Block(block) => {
                    let slot = self.order.len();
                    self.order.push(Extent {
                        lo: 0,
                        hi: 0,
                        need: 0,
                    });
                    let mut inner = Extent {
                        lo: i32::MAX,
                        hi: i32::MIN,
                        need: 0,
                    };
                    for branch in block.branches() {
                        let b = self.items(&branch.items, ctx);
                        inner.lo = inner.lo.min(b.lo);
                        inner.hi = inner.hi.max(b.hi);
                        inner.need = inner.need.max(b.need);
                        // Every branch label is a tab this frame must
                        // hold — the opener AND each `else`/`and`.
                        inner.need = inner
                            .need
                            .max(width(&tab_text(block.kind, &branch.label)) + TAB_MARGIN);
                    }
                    // A block with no contents spans every lifeline: a
                    // frame hugging nothing would read as belonging to
                    // nothing.
                    if inner.lo > inner.hi {
                        inner.lo = ctx.centers.first().copied().unwrap_or(0);
                        inner.hi = ctx.centers.last().copied().unwrap_or(0);
                    }
                    // The parent must be able to hold this child with a
                    // cell of air on each side.
                    let child_need = (inner.hi - inner.lo + 1).max(inner.need) + 2;
                    self.order[slot] = inner.clone();
                    ext.lo = ext.lo.min(inner.lo);
                    ext.hi = ext.hi.max(inner.hi);
                    ext.need = ext.need.max(child_need);
                }
                SeqItem::Activate(_) | SeqItem::Deactivate(_) => {}
            }
        }
        ext
    }
}

/// Where a self-message's label starts, relative to its lifeline.
const SELF_LABEL_X: i32 = 6;

/// The immutable context a walk reads: column geometry and the
/// participant lookup. Held here so the mutable cursor below is the
/// ONLY thing threaded through recursion.
struct Ctx<'a> {
    centers: &'a [i32],
    participants: &'a [crate::ir::Participant],
}

impl Ctx<'_> {
    fn col_of(&self, id: &str) -> usize {
        self.participants
            .iter()
            .position(|p| p.id == id)
            .unwrap_or(0)
    }

    fn center(&self, col: usize) -> i32 {
        self.centers.get(col).copied().unwrap_or(0)
    }
}

/// One participant's open activation.
struct OpenActivation {
    col: usize,
    y0: i32,
    offset: i32,
}

/// The row-placing walk: a cursor over the diagram, not a bag of
/// everything. Geometry lives in [`Ctx`], sizes in [`Extents`]; what
/// changes as rows are placed lives here.
struct Walk<'a> {
    ctx: &'a Ctx<'a>,
    extents: &'a Extents,
    /// Index of the next block's extent, in the order both walks meet
    /// them.
    next_block: usize,
    rows: Vec<RowPlan>,
    /// Next free content row.
    y: i32,
    max_right: i32,
    min_left: i32,
    /// The arrow row of the message just placed, if the last item WAS
    /// a message. `A->>+B` activates on that arrow, not three rows
    /// below it where the cursor has already moved to.
    last_arrow_y: Option<i32>,
    /// Activations opened and not yet closed, innermost last.
    active: Vec<OpenActivation>,
}

impl Walk<'_> {
    /// Place `items`; `bounds` is the frame enclosing them, if any.
    fn items(&mut self, items: &[SeqItem], depth: i32, bounds: Option<(i32, i32)>) {
        for item in items {
            match item {
                SeqItem::Message(m) => {
                    let (from_col, to_col) = (self.ctx.col_of(&m.from), self.ctx.col_of(&m.to));
                    let arrow_y = self.y + 1;
                    if from_col == to_col {
                        self.rows.push(RowPlan::SelfMessage {
                            y: self.y,
                            col: from_col,
                            kind: m.kind,
                            text: m.text.clone(),
                        });
                        self.max_right = self
                            .max_right
                            .max(self.ctx.center(from_col) + SELF_LABEL_X + width(&m.text));
                        self.y += 4;
                    } else {
                        self.rows.push(RowPlan::Message {
                            y: self.y,
                            from_col,
                            to_col,
                            kind: m.kind,
                            text: m.text.clone(),
                        });
                        self.y += 3;
                    }
                    self.last_arrow_y = Some(arrow_y);
                    continue;
                }
                SeqItem::Note(n) => {
                    let rect = note_rect(self.ctx.centers, &|id| self.ctx.col_of(id), self.y, n);
                    self.min_left = self.min_left.min(rect.x);
                    self.max_right = self.max_right.max(rect.right());
                    self.rows.push(RowPlan::Note {
                        rect,
                        text: n.text.clone(),
                    });
                    self.y += 4;
                }
                SeqItem::Activate(id) => {
                    let col = self.ctx.col_of(id);
                    let offset = self.active.iter().filter(|a| a.col == col).count() as i32;
                    // The `+` suffix expands to an Activate right after
                    // its message: the bar starts ON that arrow.
                    let y0 = self.last_arrow_y.unwrap_or(self.y);
                    self.active.push(OpenActivation { col, y0, offset });
                }
                SeqItem::Deactivate(id) => {
                    let col = self.ctx.col_of(id);
                    if let Some(pos) = self.active.iter().rposition(|a| a.col == col) {
                        let open = self.active.remove(pos);
                        let y1 = self.last_arrow_y.unwrap_or(self.y - 1);
                        self.close_activation(open, y1);
                    }
                }
                SeqItem::Block(block) => self.block(block, depth, bounds),
            }
            self.last_arrow_y = None;
        }
    }

    /// A frame: one row for the top border, the branches with their
    /// dividers between them, one row for the bottom.
    fn block(&mut self, block: &crate::ir::Block, depth: i32, bounds: Option<(i32, i32)>) {
        let ext = self.extents.order[self.next_block].clone();
        self.next_block += 1;

        // Horizontal extent FIRST, so the branches below know the
        // frame they must fit inside.
        // Clamp the LEFT edge first, then measure: sizing before
        // clamping loses a cell to the parent's border and truncates a
        // tab the parent was widened to hold.
        let pad = 1;
        let mut x0 = ext.lo - pad;
        if let Some((px0, _)) = bounds {
            // A child never touches its parent's border: the two would
            // read as one box, and the parent paints last.
            x0 = x0.max(px0 + 1);
        }
        let want = (ext.hi + pad - x0 + 1).max(ext.need);
        let mut x1 = x0 + want - 1;
        if let Some((_, px1)) = bounds {
            x1 = x1.min(px1 - 1).max(x0 + 1);
        }

        let top = self.y;
        self.y += 1;
        let mut dividers: Vec<(i32, String)> = Vec::new();
        for (i, branch) in block.branches().enumerate() {
            if i > 0 {
                dividers.push((self.y, branch.label.clone()));
                self.y += 1;
            }
            self.items(&branch.items, depth + 1, Some((x0, x1)));
        }
        let bottom = self.y;
        self.y += 1;

        self.min_left = self.min_left.min(x0);
        self.max_right = self.max_right.max(x1 + 1);
        let room = (x1 - x0 + 1 - TAB_MARGIN).max(0);
        self.rows.push(RowPlan::Frame {
            rect: Rect::new(x0, top, x1 - x0 + 1, bottom - top + 1),
            tab: truncate_ellipsis(&tab_text(block.kind, block.label()), room),
        });
        for (y, label) in dividers {
            let text = label.trim();
            let tab = if text.is_empty() {
                String::new()
            } else {
                truncate_ellipsis(&format!(" {text} "), room)
            };
            self.rows.push(RowPlan::Divider { y, x0, x1, tab });
        }
    }

    fn close_activation(&mut self, open: OpenActivation, y1: i32) {
        self.rows.push(RowPlan::Activation {
            col: open.col,
            y0: open.y0,
            y1: y1.max(open.y0),
            offset: open.offset,
        });
    }

    /// A diagram may end with participants still active (mermaid draws
    /// the bar to the bottom); close them at the last content row.
    fn close_open_activations(&mut self) {
        let end = self.y - 1;
        while let Some(open) = self.active.pop() {
            self.close_activation(open, end);
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
