//! PageHost tab-bar geometry + paint (`#[path]` sibling of
//! `page_host.rs`, file-size discipline).
//!
//! ONE pure plan (`plan_bar`) is consumed by BOTH the draw closure and
//! the click hit-test (`hit_bar`) — the console-tui screen_bar had to
//! hand-mirror its draw arithmetic inside its mouse handler
//! (ui/mod.rs:694-698 there); a single geometry source kills that
//! drift class by construction.
//!
//! Overflow model (0550's strip ruling): when the tabs exceed the
//! width, the strip WINDOWS — two columns are reserved on each side
//! for the `‹`/`›` indicators (reserved even when one side has nothing
//! hidden, so segments never reflow as indicators appear), the window
//! start is STICKY (it moves only when the active tab would leave the
//! window), and a single tab wider than the whole budget truncates its
//! title with an ellipsis.
//!
//! OWNER: TABS (wave 8, backlog 0545).

use crate::base::{Point, Rect, Rgba};
use crate::render::{Attrs, Style};
use crate::theme::TokenSet;
use crate::ui::StyledCanvas;

/// Columns reserved on each side for an overflow indicator zone.
pub(crate) const IND_ZONE: i32 = 2;
/// Gap between tab segments (Tabs precedent).
const GAP: i32 = 1;

/// One tab as the bar renders it: resolved title + resolved badge text
/// (badge getters run TRACKED in the bar's dyn build, never in draw).
pub(crate) struct BarItem {
    pub title: String,
    pub badge: Option<String>,
}

pub(crate) struct BarModel {
    pub items: Vec<BarItem>,
    pub active: usize,
}

/// A placed segment; `x` is relative to the bar rect's left edge and
/// `title_w` is the column budget the title may use (clamped titles
/// truncate with `text::truncate_ellipsis`).
/// `Clone`: the draw closure stashes the plan it painted so the mouse
/// handler can hit-test WHAT THE USER SEES (review2 F1 — a same-batch
/// badge change used to shift the geometry under a click).
#[derive(Clone)]
pub(crate) struct BarSeg {
    pub index: usize,
    pub x: i32,
    pub w: i32,
    pub title_w: i32,
}

#[derive(Clone)]
pub(crate) struct BarPlan {
    pub segs: Vec<BarSeg>,
    /// First visible tab (the sticky window anchor to persist).
    pub first: usize,
    pub overflow: bool,
    pub left_more: bool,
    pub right_more: bool,
}

impl BarPlan {
    fn empty() -> BarPlan {
        BarPlan {
            segs: Vec::new(),
            first: 0,
            overflow: false,
            left_more: false,
            right_more: false,
        }
    }
}

/// Natural segment width: " title" + (" badge")? + " ".
fn seg_w(title_w: i32, badge_w: i32) -> i32 {
    1 + title_w + if badge_w > 0 { 1 + badge_w } else { 0 } + 1
}

/// Compute the bar plan for `avail` columns. Pure: same inputs, same
/// plan — the draw closure and the mouse handler both call it.
pub(crate) fn plan_bar(m: &BarModel, prev_first: usize, avail: i32) -> BarPlan {
    let n = m.items.len();
    if n == 0 || avail <= 0 {
        return BarPlan::empty();
    }
    let tw: Vec<i32> = m
        .items
        .iter()
        .map(|i| crate::text::width(&i.title))
        .collect();
    let bw: Vec<i32> = m
        .items
        .iter()
        .map(|i| i.badge.as_deref().map(crate::text::width).unwrap_or(0))
        .collect();

    let total: i32 = (0..n).map(|i| seg_w(tw[i], bw[i])).sum::<i32>() + GAP * (n as i32 - 1);
    if total <= avail {
        let mut segs = Vec::with_capacity(n);
        let mut x = 0;
        for i in 0..n {
            let w = seg_w(tw[i], bw[i]);
            segs.push(BarSeg {
                index: i,
                x,
                w,
                title_w: tw[i],
            });
            x += w + GAP;
        }
        return BarPlan {
            segs,
            first: 0,
            overflow: false,
            left_more: false,
            right_more: false,
        };
    }

    // Overflow: window the strip inside the indicator-reserved budget.
    let budget = (avail - 2 * IND_ZONE).max(4);
    // Clamp any single oversized tab so it can fit the budget alone;
    // the title takes the cut (badges stay whole; a badge wider than
    // the budget is clipped at draw time — degenerate by then).
    let title_w: Vec<i32> = (0..n)
        .map(|i| {
            if seg_w(tw[i], bw[i]) > budget {
                (budget - seg_w(0, bw[i])).max(1)
            } else {
                tw[i]
            }
        })
        .collect();
    let active = m.active.min(n - 1);
    let run_w = |a: usize, b: usize| -> i32 {
        (a..=b).map(|i| seg_w(title_w[i], bw[i])).sum::<i32>() + GAP * (b - a) as i32
    };
    // Sticky start: keep the previous window anchor when the active tab
    // is still reachable from it; otherwise slide minimally.
    let mut first = prev_first.min(active);
    while first < active && run_w(first, active) > budget {
        first += 1;
    }
    let mut last = active;
    while last + 1 < n && run_w(first, last + 1) <= budget {
        last += 1;
    }
    let mut segs = Vec::with_capacity(last - first + 1);
    let mut x = IND_ZONE;
    for i in first..=last {
        let w = seg_w(title_w[i], bw[i]);
        segs.push(BarSeg {
            index: i,
            x,
            w,
            title_w: title_w[i],
        });
        x += w + GAP;
    }
    BarPlan {
        segs,
        first,
        overflow: true,
        left_more: first > 0,
        right_more: last < n - 1,
    }
}

/// What a press at bar-relative column `x` means.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BarHit {
    Prev,
    Next,
    Tab(usize),
    Miss,
}

pub(crate) fn hit_bar(plan: &BarPlan, avail: i32, x: i32) -> BarHit {
    if plan.overflow {
        if x < IND_ZONE {
            return if plan.left_more {
                BarHit::Prev
            } else {
                BarHit::Miss
            };
        }
        if x >= avail - IND_ZONE {
            return if plan.right_more {
                BarHit::Next
            } else {
                BarHit::Miss
            };
        }
    }
    for s in &plan.segs {
        if x >= s.x && x < s.x + s.w {
            return BarHit::Tab(s.index);
        }
    }
    BarHit::Miss
}

/// Resolved inks (style-guide §3.3, tokens only — RT1-9b).
pub(crate) struct BarInk {
    pub active: Rgba,
    pub idle: Rgba,
    pub badge: Rgba,
    pub strip: Rgba,
    pub indicator: Rgba,
    pub ground: Rgba,
}

pub(crate) fn ink_from(t: &TokenSet) -> BarInk {
    BarInk {
        active: t.text,
        idle: t.text_muted,
        badge: t.info,
        strip: t.border_focus,
        indicator: t.text_muted,
        ground: t.surface,
    }
}

/// Paint the two bar rows (titles+badges, then the active cell strip).
/// Draw-closure discipline: everything here is captured/derived data —
/// no signal reads (RT1-2).
pub(crate) fn draw_bar(
    canvas: &mut dyn StyledCanvas,
    rect: Rect,
    m: &BarModel,
    plan: &BarPlan,
    ink: &BarInk,
) {
    if rect.is_empty() {
        return;
    }
    canvas.fill_styled(rect, ' ', &Style::new().fg(ink.idle).bg(ink.ground));
    // The column past which nothing may paint (right indicator zone).
    let limit = if plan.overflow {
        rect.w - IND_ZONE
    } else {
        rect.w
    };
    if plan.overflow {
        let ind = Style::new().fg(ink.indicator).bg(ink.ground);
        if plan.left_more {
            canvas.print_styled(Point::new(rect.x, rect.y), "‹", &ind);
        }
        if plan.right_more && rect.w >= 1 {
            canvas.print_styled(Point::new(rect.x + rect.w - 1, rect.y), "›", &ind);
        }
    }
    for s in &plan.segs {
        if s.x >= limit {
            break;
        }
        let item = &m.items[s.index];
        let active = s.index == m.active;
        let title_style = if active {
            Style::new()
                .fg(ink.active)
                .bg(ink.ground)
                .attrs(Attrs::BOLD)
        } else {
            Style::new().fg(ink.idle).bg(ink.ground)
        };
        let title_cols = s.title_w.min(limit - s.x - 1);
        let title = crate::text::truncate_ellipsis(&item.title, title_cols);
        canvas.print_styled(Point::new(rect.x + s.x + 1, rect.y), &title, &title_style);
        if let Some(b) = &item.badge {
            let bx = s.x + 1 + s.title_w + 1;
            if bx < limit {
                let badge = crate::text::truncate_ellipsis(b, limit - bx);
                canvas.print_styled(
                    Point::new(rect.x + bx, rect.y),
                    &badge,
                    &Style::new().fg(ink.badge).bg(ink.ground),
                );
            }
        }
        if active && rect.h > 1 {
            // The strip spans the segment, clipped to the paint limit.
            let strip_w = s.w.min(limit - s.x).max(0);
            let strip = "▔".repeat(strip_w as usize);
            canvas.print_styled(
                Point::new(rect.x + s.x, rect.y + 1),
                &strip,
                &Style::new().fg(ink.strip).bg(ink.ground),
            );
        }
    }
}
