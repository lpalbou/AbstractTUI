//! The flexbox-subset solver: pure geometry, integer cells, deterministic.
//!
//! Contract: `solve(tree, root, container)` assigns every node an absolute
//! `Rect`. Re-solving any subtree whose box did not change produces
//! identical output (purity is what makes incremental re-solve and damage
//! diffing trustworthy). When grow fills a container, children tile it
//! EXACTLY — rounding is distributed by largest remainder, never dropped.

use crate::base::{Point, Rect, Size};

use super::flex_math::{distribute, resolve_main_sizes, FlexItem};
use super::style::{Align, Dimension, Direction, Justify, Position};
use super::tree::{LayoutId, LayoutTree};

/// Solve the whole tree: `root` fills `container` (the viewport or the
/// parent-assigned box), children flow inside it.
pub fn solve(tree: &mut LayoutTree, root: LayoutId, container: Rect) {
    if !tree.is_alive(root) {
        return;
    }
    tree.assign_rect(root, container);
    resolve_subtree(tree, root);
}

/// Re-solve the children of `id` given its CURRENT rect. This is the
/// incremental entry point: a `Dyn` swap that cannot change its own box
/// only needs its subtree re-solved.
pub fn resolve_subtree(tree: &mut LayoutTree, id: LayoutId) {
    // Iterative: UI nesting depth must not be bounded by the native stack.
    let mut work = vec![id];
    while let Some(current) = work.pop() {
        layout_children_of(tree, current);
        let children: Vec<LayoutId> = tree.children(current).to_vec();
        work.extend(children);
    }
}

/// Resolve a `Dimension` against a parent extent.
pub(super) fn resolve_dim(dim: Dimension, parent_extent: i32) -> Option<i32> {
    match dim {
        Dimension::Auto => None,
        Dimension::Cells(c) => Some(c.max(0)),
        Dimension::Percent(p) => Some(((parent_extent as f32) * p.clamp(0.0, 1.0)).round() as i32),
    }
}

pub(super) fn clamp_axis(v: i32, min: Option<i32>, max: Option<i32>) -> i32 {
    let lo = min.unwrap_or(0).max(0);
    let hi = max.unwrap_or(i32::MAX).max(lo);
    v.clamp(lo, hi)
}

/// THE size query (backlog 0130): measure a subtree's intrinsic size
/// within `avail` without assigning rects — what a scroll container
/// asks to size its content wrapper when no `content_size` hint is
/// given. Explicit dimensions win; `Auto` asks the measure callback
/// (leaves) or aggregates children. Pure: no tree mutation.
///
/// Unconstrained axes pass a large `avail` extent (the solver's
/// absolute-placement path does exactly this through the same fold).
pub fn measure(tree: &LayoutTree, id: LayoutId, avail: Size) -> Size {
    intrinsic_size(tree, id, avail)
}

/// Content-driven size of a node within `avail`. Explicit dimensions win;
/// `Auto` asks the measure callback (leaves) or aggregates children
/// (containers): sum along the node's own main axis, max across.
///
/// Recursive over tree depth; fine for realistic UI nesting. Percent
/// resolves against `avail` (the parent content estimate) — good enough
/// for intrinsic purposes and documented as an approximation.
pub(super) fn intrinsic_size(tree: &LayoutTree, id: LayoutId, avail: Size) -> Size {
    let Some(node) = tree.nodes.get(id.0) else {
        return Size::ZERO;
    };
    let style = &node.style;
    let explicit_w = resolve_dim(style.width, avail.w);
    let explicit_h = resolve_dim(style.height, avail.h);
    let need_content = explicit_w.is_none() || explicit_h.is_none();
    let content = if !need_content {
        Size::ZERO
    } else if let Some(measure) = &node.measure {
        let inner = Size::new(
            (avail.w - style.padding.horizontal()).max(0),
            (avail.h - style.padding.vertical()).max(0),
        );
        let m = measure(inner);
        Size::new(
            m.w + style.padding.horizontal(),
            m.h + style.padding.vertical(),
        )
    } else {
        let inner_avail = Size::new(
            (avail.w - style.padding.horizontal()).max(0),
            (avail.h - style.padding.vertical()).max(0),
        );
        let mut main = 0i32;
        let mut cross = 0i32;
        let mut flow_children = 0;
        for &child in &node.children {
            let Some(cnode) = tree.nodes.get(child.0) else {
                continue;
            };
            if cnode.style.position == Position::Absolute {
                continue; // out of flow: contributes nothing to content size
            }
            let cs = intrinsic_size(tree, child, inner_avail);
            let (cm, cc) = match style.direction {
                Direction::Row => (
                    cs.w + cnode.style.margin.horizontal(),
                    cs.h + cnode.style.margin.vertical(),
                ),
                Direction::Column => (
                    cs.h + cnode.style.margin.vertical(),
                    cs.w + cnode.style.margin.horizontal(),
                ),
            };
            main += cm;
            cross = cross.max(cc);
            flow_children += 1;
        }
        if flow_children > 1 {
            main += style.gap.max(0) * (flow_children - 1);
        }
        match style.direction {
            Direction::Row => Size::new(
                main + style.padding.horizontal(),
                cross + style.padding.vertical(),
            ),
            Direction::Column => Size::new(
                cross + style.padding.horizontal(),
                main + style.padding.vertical(),
            ),
        }
    };
    Size::new(
        clamp_axis(
            explicit_w.unwrap_or(content.w),
            style.min_width,
            style.max_width,
        ),
        clamp_axis(
            explicit_h.unwrap_or(content.h),
            style.min_height,
            style.max_height,
        ),
    )
}

/// Axis view helpers: the solver is written once, axis-agnostic.
#[derive(Copy, Clone)]
struct Axes {
    dir: Direction,
}

impl Axes {
    fn main(self, s: Size) -> i32 {
        match self.dir {
            Direction::Row => s.w,
            Direction::Column => s.h,
        }
    }
    fn size(self, main: i32, cross: i32) -> Size {
        match self.dir {
            Direction::Row => Size::new(main, cross),
            Direction::Column => Size::new(cross, main),
        }
    }
}

fn layout_children_of(tree: &mut LayoutTree, id: LayoutId) {
    let (rect, style, children) = {
        let Some(node) = tree.nodes.get(id.0) else {
            return;
        };
        (node.rect, node.style.clone(), node.children.clone())
    };
    if children.is_empty() {
        return;
    }
    let content = Rect::new(
        rect.x + style.padding.left,
        rect.y + style.padding.top,
        (rect.w - style.padding.horizontal()).max(0),
        (rect.h - style.padding.vertical()).max(0),
    );
    let axes = Axes {
        dir: style.direction,
    };
    let content_main = axes.main(content.size());
    let mut flow: Vec<LayoutId> = Vec::new();
    let mut absolute: Vec<LayoutId> = Vec::new();
    for child in children {
        let Some(cnode) = tree.nodes.get(child.0) else {
            continue;
        };
        if cnode.style.position == Position::Absolute {
            absolute.push(child);
        } else {
            flow.push(child);
        }
    }

    // Non-flex algorithms take over the FLOW children; absolute
    // placement below is shared by every display mode.
    if let super::style::Display::Grid { .. } = &style.display {
        super::grid::layout_grid(tree, content, &style, &flow);
        place_absolute(tree, &absolute, content);
        return;
    }
    if style.wrap {
        super::wrap::layout_wrapped(tree, content, &style, &flow);
        place_absolute(tree, &absolute, content);
        return;
    }

    // ---- main axis ----------------------------------------------------
    let gap = style.gap.max(0);
    let gaps_total = if flow.len() > 1 {
        gap * (flow.len() as i32 - 1)
    } else {
        0
    };
    let mut items: Vec<FlexItem> = Vec::with_capacity(flow.len());
    let mut margins_main: Vec<(i32, i32)> = Vec::with_capacity(flow.len());
    // Zero-collapse watch (0240 follow-up #3): per child, the DECLARED
    // fixed main-axis extent (explicit `Cells` basis, else explicit
    // `Cells` size; 0 = not watched). An author-declared min — even
    // min 0, the documented opt-out — unwatches; percent/intrinsic
    // sizing is legitimately parent-relative and never watched.
    let mut declared_fixed: Vec<i32> = Vec::with_capacity(flow.len());
    for &child in &flow {
        let cstyle = tree
            .nodes
            .get(child.0)
            .expect("flow child alive")
            .style
            .clone();
        let (m_lead, m_trail, min, max, main_dim) = match style.direction {
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
        declared_fixed.push(match (min, cstyle.basis, main_dim) {
            (Some(_), _, _) => 0, // explicit min = the author decided
            // Mirror basis precedence: an explicit basis (even the
            // deliberate `Cells(0)` of scroll containers) overrides
            // the fixed size, so only a POSITIVE Cells basis watches.
            (None, Dimension::Cells(n), _) => n.max(0),
            (None, Dimension::Auto, Dimension::Cells(n)) => n.max(0),
            _ => 0,
        });
        // flex-basis > explicit main size > intrinsic content.
        let basis = resolve_dim(cstyle.basis, content_main)
            .or_else(|| resolve_dim(main_dim, content_main))
            .unwrap_or_else(|| {
                let est = intrinsic_size(tree, child, content.size());
                axes.main(est)
            });
        items.push(FlexItem {
            basis: basis.max(0),
            min: min.unwrap_or(0).max(0),
            max: max.unwrap_or(i32::MAX),
            grow: cstyle.grow.max(0.0) as f64,
            shrink: cstyle.shrink.max(0.0) as f64,
        });
        margins_main.push((m_lead, m_trail));
    }
    let margins_total: i32 = margins_main.iter().map(|(a, b)| a + b).sum();
    let available_main = (content_main - gaps_total - margins_total).max(0);
    let sizes = resolve_main_sizes(&items, available_main);

    // Debug diagnostic (0240 follow-up #3): a watched fixed-size child
    // crushed to zero names itself once instead of silently vanishing
    // (const-folded away in release builds).
    if cfg!(debug_assertions) {
        let axis = match style.direction {
            Direction::Row => "width",
            Direction::Column => "height",
        };
        for (i, &child) in flow.iter().enumerate() {
            if declared_fixed[i] > 0 && sizes[i] == 0 {
                tree.note_zero_collapse(child, declared_fixed[i], axis, content, i);
            }
        }
    }

    // Leftover space feeds `justify` (only exists when nothing grew).
    let used: i32 = sizes.iter().sum();
    let leftover = (available_main - used).max(0);
    let (lead_offset, between_extra) = match style.justify {
        Justify::Start => (0, vec![0; flow.len().saturating_sub(1)]),
        Justify::Center => (leftover / 2, vec![0; flow.len().saturating_sub(1)]),
        Justify::End => (leftover, vec![0; flow.len().saturating_sub(1)]),
        Justify::SpaceBetween => {
            let slots = flow.len().saturating_sub(1);
            if slots == 0 {
                // Single child: SpaceBetween degenerates to Start (CSS).
                (0, Vec::new())
            } else {
                // Integer leftover split across the inter-child gaps,
                // largest-remainder so the row still tiles exactly.
                (0, distribute(leftover, &vec![1.0; slots]))
            }
        }
    };

    // ---- place children ------------------------------------------------
    let content_cross_extent = match style.direction {
        Direction::Row => content.h,
        Direction::Column => content.w,
    };
    let mut cursor = match style.direction {
        Direction::Row => content.x,
        Direction::Column => content.y,
    } + lead_offset;
    for (i, &child) in flow.iter().enumerate() {
        let cstyle = tree
            .nodes
            .get(child.0)
            .expect("flow child alive")
            .style
            .clone();
        let main_size = sizes[i];
        let (m_lead, _m_trail) = margins_main[i];

        // Cross axis: explicit > stretch > intrinsic; then clamp + align.
        let (cross_dim, cross_min, cross_max, m_cross_lead, m_cross_total) = match style.direction {
            Direction::Row => (
                cstyle.height,
                cstyle.min_height,
                cstyle.max_height,
                cstyle.margin.top,
                cstyle.margin.vertical(),
            ),
            Direction::Column => (
                cstyle.width,
                cstyle.min_width,
                cstyle.max_width,
                cstyle.margin.left,
                cstyle.margin.horizontal(),
            ),
        };
        let cross_avail = (content_cross_extent - m_cross_total).max(0);
        let align = cstyle.align_self.unwrap_or(style.align_items);
        let cross_size = match resolve_dim(cross_dim, content_cross_extent) {
            Some(c) => c,
            None => match align {
                Align::Stretch => cross_avail,
                _ => {
                    let est = intrinsic_size(tree, child, axes.size(main_size, cross_avail));
                    match style.direction {
                        Direction::Row => est.h,
                        Direction::Column => est.w,
                    }
                }
            },
        };
        let cross_size = clamp_axis(cross_size, cross_min, cross_max).min(cross_avail.max(0));
        let cross_offset = match align {
            Align::Start | Align::Stretch => 0,
            Align::Center => (cross_avail - cross_size) / 2,
            Align::End => cross_avail - cross_size,
        };

        cursor += m_lead;
        let (x, y, w, h) = match style.direction {
            Direction::Row => (
                cursor,
                content.y + m_cross_lead + cross_offset,
                main_size,
                cross_size,
            ),
            Direction::Column => (
                content.x + m_cross_lead + cross_offset,
                cursor,
                cross_size,
                main_size,
            ),
        };
        tree.assign_rect(child, Rect::new(x, y, w.max(0), h.max(0)));
        cursor += main_size + margins_main[i].1;
        if i + 1 < flow.len() {
            cursor += gap + between_extra.get(i).copied().unwrap_or(0);
        }
    }

    // ---- absolute children ----------------------------------------------
    place_absolute(tree, &absolute, content);
}

/// Out-of-flow placement against the parent's content box — shared by
/// flex, wrap and grid containers.
pub(super) fn place_absolute(tree: &mut LayoutTree, absolute: &[LayoutId], content: Rect) {
    for &child in absolute {
        let cstyle = tree
            .nodes
            .get(child.0)
            .expect("absolute child alive")
            .style
            .clone();
        let inset = cstyle.inset;
        let est = intrinsic_size(tree, child, content.size());
        // Horizontal: both insets + Auto width => width derived from insets.
        let w = match resolve_dim(cstyle.width, content.w) {
            Some(w) => w,
            None => match (inset.left, inset.right) {
                (Some(l), Some(r)) => (content.w - l - r).max(0),
                _ => est.w,
            },
        };
        let h = match resolve_dim(cstyle.height, content.h) {
            Some(h) => h,
            None => match (inset.top, inset.bottom) {
                (Some(t), Some(b)) => (content.h - t - b).max(0),
                _ => est.h,
            },
        };
        let w = clamp_axis(w, cstyle.min_width, cstyle.max_width);
        let h = clamp_axis(h, cstyle.min_height, cstyle.max_height);
        let x = match (inset.left, inset.right) {
            (Some(l), _) => content.x + l,
            (None, Some(r)) => content.right() - r - w,
            (None, None) => content.x,
        };
        let y = match (inset.top, inset.bottom) {
            (Some(t), _) => content.y + t,
            (None, Some(b)) => content.bottom() - b - h,
            (None, None) => content.y,
        };
        tree.assign_rect(child, Rect::new(x, y, w.max(0), h.max(0)));
    }
}

/// Convenience: place `p` relative to a solved rect. PRUNED from the
/// public surface (cycle-8 sweep: zero external consumers); widgets do
/// this arithmetic inline.
#[allow(dead_code)]
pub(crate) fn local_point(rect: Rect, p: Point) -> Point {
    Point::new(p.x - rect.x, p.y - rect.y)
}
