//! Flex WRAP: children that overflow the main axis start a new line.
//!
//! Model (the CSS flex-wrap subset that makes sense in cells): lines
//! break greedily on the sum of flex BASES (+ margins + gaps) against
//! the container's main extent — at least one child per line, so a
//! too-wide child gets its own line instead of an infinite loop. Each
//! line then distributes grow/shrink independently over the SAME
//! largest-remainder math as a non-wrapped row (a line is a row). Lines
//! stack along the cross axis, separated by `cross_gap`; each line's
//! cross extent is the max of its children's cross sizes, and Stretch
//! children fill their LINE, not the container (CSS `align-items`
//! within `align-content: start` — the only align-content mode in v1).
//!
//! Purity contract as everywhere in the solver: same inputs, same
//! rects; integer cells; nothing dropped to rounding.

use crate::base::{Rect, Size};

use super::flex_math::{resolve_main_sizes, FlexItem};
use super::solve::{clamp_axis, intrinsic_size, resolve_dim};
use super::style::{Align, Direction, Style};
use super::tree::{LayoutId, LayoutTree};

pub(super) fn layout_wrapped(
    tree: &mut LayoutTree,
    content: Rect,
    style: &Style,
    flow: &[LayoutId],
) {
    if flow.is_empty() {
        return;
    }
    let dir = style.direction;
    let gap = style.gap.max(0);
    let cross_gap = style.cross_gap.max(0);
    let content_main = match dir {
        Direction::Row => content.w,
        Direction::Column => content.h,
    };

    // Per-child flex inputs (same rules as the single-line path).
    struct ChildIn {
        id: LayoutId,
        item: FlexItem,
        m_lead: i32,
        m_trail: i32,
    }
    let mut inputs: Vec<ChildIn> = Vec::with_capacity(flow.len());
    for &child in flow {
        let cstyle = tree
            .nodes
            .get(child.0)
            .expect("flow child alive")
            .style
            .clone();
        let (m_lead, m_trail, min, max, main_dim) = match dir {
            Direction::Row => (
                cstyle.margin.left,
                cstyle.margin.right,
                cstyle.min_width,
                cstyle.max_width,
                cstyle.width,
            ),
            Direction::Column => (
                cstyle.margin.top,
                cstyle.margin.bottom,
                cstyle.min_height,
                cstyle.max_height,
                cstyle.height,
            ),
        };
        let basis = resolve_dim(cstyle.basis, content_main)
            .or_else(|| resolve_dim(main_dim, content_main))
            .unwrap_or_else(|| {
                let est = intrinsic_size(tree, child, content.size());
                match dir {
                    Direction::Row => est.w,
                    Direction::Column => est.h,
                }
            });
        inputs.push(ChildIn {
            id: child,
            item: FlexItem {
                basis: basis.max(0),
                min: min.unwrap_or(0).max(0),
                max: max.unwrap_or(i32::MAX),
                grow: cstyle.grow.max(0.0) as f64,
                shrink: cstyle.shrink.max(0.0) as f64,
            },
            m_lead,
            m_trail,
        });
    }

    // Greedy line breaking on hypothetical (basis) sizes.
    let mut lines: Vec<std::ops::Range<usize>> = Vec::new();
    let mut start = 0usize;
    let mut used = 0i32;
    for (i, c) in inputs.iter().enumerate() {
        let need = c.item.basis.clamp(c.item.min, c.item.max) + c.m_lead + c.m_trail;
        let with_gap = if i > start { need + gap } else { need };
        if i > start && used + with_gap > content_main {
            lines.push(start..i);
            start = i;
            used = need;
        } else {
            used += with_gap;
        }
    }
    lines.push(start..inputs.len());

    // Lay each line out as an independent row, stacked on the cross axis.
    let mut cross_cursor = match dir {
        Direction::Row => content.y,
        Direction::Column => content.x,
    };
    for (li, line) in lines.iter().enumerate() {
        let members = &inputs[line.clone()];
        let gaps_total = gap * (members.len() as i32 - 1).max(0);
        let margins_total: i32 = members.iter().map(|c| c.m_lead + c.m_trail).sum();
        let avail = (content_main - gaps_total - margins_total).max(0);
        let items: Vec<FlexItem> = members.iter().map(|c| c.item).collect();
        let sizes = resolve_main_sizes(&items, avail);

        // First pass: cross sizes (Stretch resolves after the line
        // extent is known, from the non-stretch members' maximum).
        let mut cross_sizes: Vec<Option<i32>> = Vec::with_capacity(members.len());
        let mut line_extent = 0i32;
        for (i, c) in members.iter().enumerate() {
            let cstyle = tree.nodes.get(c.id.0).expect("alive").style.clone();
            let (cross_dim, cmin, cmax, m_cross_total) = match dir {
                Direction::Row => (
                    cstyle.height,
                    cstyle.min_height,
                    cstyle.max_height,
                    cstyle.margin.vertical(),
                ),
                Direction::Column => (
                    cstyle.width,
                    cstyle.min_width,
                    cstyle.max_width,
                    cstyle.margin.horizontal(),
                ),
            };
            let align = cstyle.align_self.unwrap_or(style.align_items);
            let content_cross = match dir {
                Direction::Row => content.h,
                Direction::Column => content.w,
            };
            let resolved = match resolve_dim(cross_dim, content_cross) {
                Some(c) => Some(c),
                None if align == Align::Stretch => None, // fills the line later
                None => {
                    let est = intrinsic_size(
                        tree,
                        c.id,
                        match dir {
                            Direction::Row => Size::new(sizes[i], content.h),
                            Direction::Column => Size::new(content.w, sizes[i]),
                        },
                    );
                    Some(match dir {
                        Direction::Row => est.h,
                        Direction::Column => est.w,
                    })
                }
            }
            .map(|c| clamp_axis(c, cmin, cmax));
            if let Some(c) = resolved {
                line_extent = line_extent.max(c + m_cross_total);
            }
            cross_sizes.push(resolved);
        }
        if line_extent == 0 {
            // All-stretch line: fall back to intrinsic so the line has
            // real height (a zero-tall line would vanish).
            for (i, c) in members.iter().enumerate() {
                let est = intrinsic_size(tree, c.id, content.size());
                let e = match dir {
                    Direction::Row => est.h,
                    Direction::Column => est.w,
                };
                line_extent = line_extent.max(e);
                let _ = i;
            }
        }

        // Second pass: place.
        let mut main_cursor = match dir {
            Direction::Row => content.x,
            Direction::Column => content.y,
        };
        for (i, c) in members.iter().enumerate() {
            let cstyle = tree.nodes.get(c.id.0).expect("alive").style.clone();
            let (m_cross_lead, m_cross_total) = match dir {
                Direction::Row => (cstyle.margin.top, cstyle.margin.vertical()),
                Direction::Column => (cstyle.margin.left, cstyle.margin.horizontal()),
            };
            let align = cstyle.align_self.unwrap_or(style.align_items);
            let cross_avail = (line_extent - m_cross_total).max(0);
            let cross = cross_sizes[i]
                .unwrap_or(cross_avail)
                .min(cross_avail.max(0));
            let cross_offset = match align {
                Align::Start | Align::Stretch => 0,
                Align::Center => (cross_avail - cross) / 2,
                Align::End => cross_avail - cross,
            };
            main_cursor += c.m_lead;
            let rect = match dir {
                Direction::Row => Rect::new(
                    main_cursor,
                    cross_cursor + m_cross_lead + cross_offset,
                    sizes[i].max(0),
                    cross.max(0),
                ),
                Direction::Column => Rect::new(
                    cross_cursor + m_cross_lead + cross_offset,
                    main_cursor,
                    cross.max(0),
                    sizes[i].max(0),
                ),
            };
            tree.assign_rect(c.id, rect);
            main_cursor += sizes[i] + c.m_trail;
            if i + 1 < members.len() {
                main_cursor += gap;
            }
        }
        cross_cursor += line_extent;
        if li + 1 < lines.len() {
            cross_cursor += cross_gap;
        }
    }
}
